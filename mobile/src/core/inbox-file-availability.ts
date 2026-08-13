import { File } from "expo-file-system";
import type {
  MobileInboxFileEntry as InboxFileEntry,
  MobileTransferFile as TransferFile,
} from "react-native-swarmdrop-core";

/**
 * 已接收文件的「还在不在原位」判定。
 *
 * 落点治本之后这件事变得更常见而不是更少见：接收目录现在对用户可见，用户可以在系统
 * 文件管理器里删掉、移走、重命名收到的文件——这是正确的所有权语义（文件归用户），
 * 代价就是每个要碰它的入口（打开 / 分享 / 转发）都得先问一句「它还在吗」。
 */

/**
 * expo-fs 的 `File` 对 `file://` 与 SAF document URI 都能查 `exists`。
 *
 * 三态返回是刻意的：`true` = 存在，`false` = **确定**不存在，`null` = 查询本身失败
 * （provider 瞬时故障、授权状态未知）。
 */
export function fileExists(localPath: string): boolean | null {
  try {
    return new File(localPath).exists;
  } catch {
    return null;
  }
}

export class MissingFileError extends Error {
  constructor() {
    super("missing inbox file");
  }
}

/**
 * 这个文件此刻能不能用。
 *
 * **只有「明确查到不存在」才算不可用。** `missing` 是持久化且无解除路径的单向标记
 * （`repairMissingInboxItems` 只补建丢失的条目，不清文件级 missing），一次 SAF 抖动
 * 若被判成缺失，就会永久锁死一个完好的文件。查询失败宁可当它还在，让真实读取去暴露问题。
 *
 * 这与接收落点的 `revoked` 判定正好相反——那边探测抛错也判失效，因为它每次重算、不落库，
 * 误判只是一次多余提示。**取舍不同的根源是标记会不会留下。**
 *
 * 这条规则只有这一个实现，`ensureAvailable`（打开 / 分享）与 `selectForwardable`（转发）
 * 都从这里取——它们此前各写各的，靠注释宣称一致。
 */
export function isAvailable(file: InboxFileEntry): boolean {
  return !file.missing && fileExists(file.localPath) !== false;
}

/** 用文件前的存在性闸。不可用时抛 {@link MissingFileError}。 */
export function ensureAvailable(file: InboxFileEntry): void {
  if (!isAvailable(file)) throw new MissingFileError();
}

export function isMissingFileError(
  err: unknown,
  file: InboxFileEntry,
): boolean {
  if (err instanceof MissingFileError) return true;
  // 不靠错误文案判断（本地化 / 不同平台下英文子串会漏判）：复查文件是否还在原位。
  return fileExists(file.localPath) === false;
}

/**
 * 已接收文件 → 发送载荷。
 *
 * **`sourceId` 直接用 `localPath`，不复制、不引入新的文件源类型。** 那个值是
 * `finalize_sink` 回报的真实落盘位置（`file://` 或 SAF document URI），而
 * `ForeignFileAccess.readSourceChunk` 是 `new File(sourceId).open(ReadOnly)`——对两种
 * 形态一视同仁。所以转发在后端是零改动的：缺的从来只是一个入口。
 *
 * **不要改成 `join(saveDir, relativePath)` 推导。** SAF 下拼出的是缺 `/document/<docId>`
 * 段的伪 URI，ContentResolver 解析不了；重名被系统改写成 `foo (1).jpg` 时同样失真
 * （toolchain.md 的 vivo 真机实证）。
 */
function toTransferFile(file: InboxFileEntry): TransferFile {
  return {
    sourceId: file.localPath,
    name: file.name,
    // 平铺文件名而非 `relativePath`：转发是一次新的发送，收到的文件此刻是一批独立的
    // 文件，把上一次传输的目录结构带给第三台设备只会让对方莫名其妙。
    relativePath: file.name,
    size: file.size,
  };
}

export interface ForwardSelection {
  /** 可以发送的文件。 */
  files: TransferFile[];
  /** 已不在原位置的条目——调用方负责标记 missing 并告知用户。 */
  missing: InboxFileEntry[];
}

/**
 * 挑出真正能发的那些。
 *
 * 在**发起前**而不是传输中途做这件事：`prepareSend` 会为每个文件算 BLAKE3，一个死 URI
 * 会让整批准备失败，用户得到的却是一条与「哪个文件没了」无关的错误。
 */
export function selectForwardable(
  entries: readonly InboxFileEntry[],
): ForwardSelection {
  const files: TransferFile[] = [];
  const missing: InboxFileEntry[] = [];
  for (const entry of entries) {
    if (isAvailable(entry)) files.push(toTransferFile(entry));
    else missing.push(entry);
  }
  return { files, missing };
}
