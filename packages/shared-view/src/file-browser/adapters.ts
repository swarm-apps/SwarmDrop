/**
 * 四种数据来源 → 统一的 `FileBrowserItem[]`。
 *
 * 蓝本是 `mobile/src/components/file-browser/adapters.ts`：它已经把「进度是投影之上的
 * **覆盖层**」这件事做对了，也有一张完整的 `phase × terminalReason → status` 映射。
 * 桌面那份只有一个粗粒度的 `defaultStatus` 回退，Web 那份更是把进度与投影当成二选一
 * （`live?.files ?? projection.files`），还得靠消费点现场嗅探形状。
 *
 * ## 入参一律结构化，不 import 任何一端的 bindings
 *
 * 三端的传输类型由三条 codegen 产出（tauri-specta / wasm-bindgen / uniffi）。本包若 import
 * 其中任一，就同时把该端的构建产物变成另外两端的依赖。所以这里只声明**用到的字段**，
 * 三端各自的类型都能结构化赋值。
 *
 * 枚举同样只声明字面量，**拼写逐字沿用内核**（`waiting_accept` / `fatal_error` 这类
 * snake_case 就是 wire 的写法）。桌面与 Web 的 codegen 产出的正是同一个字符串联合，
 * 结构上直传即可；只有移动端要在自己的 adapter 里做一次映射（uniffi 给的是 enum 对象）。
 *
 * **别为了「更中立」给它们改名**：那不会让本包更平台无关（它照样得认识那套取值），
 * 只会让两个直传的端各留一段逐字相同的改名 switch。这里试过一次（`fatal_error → failed`），
 * 代价就是那两段，已回退。
 */

import { calcPercent } from "../format/quantity";
import {
  inboxFileId,
  normalizeRelativePath,
  selectedFileId,
  sessionFileId,
} from "./identity";
import type { FileBrowserItem, FileBrowserStatus } from "./types";

/* ─── 中立入参 ─── */

/** 会话生命周期阶段（中立字面量，与内核的 `TransferPhase` 同义）。 */
export type SessionPhase =
  | "offered"
  | "waiting_accept"
  | "active"
  | "suspended"
  | "terminal";

/**
 * 终态原因。**逐字沿用内核 `TerminalReason` 的拼写**，包括 `fatal_error`。
 *
 * 曾把第五个改名成 `failed`（"更中立"），代价是桌面与 Web 各留一段 16 行、逐字相同的
 * 改名 switch——而它们吃的本来就是同一个 wire 字符串联合，结构上可以直传。四个原名照抄、
 * 只改第五个，union 内部也不自洽。移动端那份映射留着，它是真的需要（uniffi 给的是 enum 对象）。
 */
export type SessionTerminalReason =
  | "completed"
  | "cancelled"
  | "rejected"
  | "expired"
  | "fatal_error";

export interface SelectedFileInput {
  /** 来源路径 / URI，同时用作展示 ID 的键。 */
  sourceId: string;
  name: string;
  relativePath: string;
  size: number;
  /** 缩略图取图源（可选）。发送侧通常就是 `sourceId` 本身。 */
  previewSource?: string;
}

export interface OfferFileInput {
  fileId: number;
  name: string;
  relativePath?: string | null;
  size: number;
  isDirectory?: boolean;
}

export interface ProjectionFileInput {
  fileId: number;
  name: string;
  relativePath: string;
  size: number;
  transferredBytes: number;
}

/**
 * 内核给的逐文件状态。**是个真枚举，不是 `string`**——此前这里开着 `string` 的口子，
 * 于是 `projectionFileStatus` 里比对的 `"failed"` / `"error"` 两个分支谁也没发现它们在
 * 三端 codegen 里根本不存在（那是从移动端旧实现整段搬来的死代码）。内核改这个枚举时，
 * 现在编译期就会红。
 */
export type ProgressFileStatus = "pending" | "transferring" | "completed";

export interface ProgressFileInput {
  fileId: number;
  transferred: number;
  status?: ProgressFileStatus;
}

export interface InboxFileInput {
  /** 收件箱文件行的主键。 */
  id: number;
  /** 关联的传输文件号（缺失时回落 `id`）。 */
  transferFileId?: number | null;
  name: string;
  relativePath: string;
  size: number;
  missing?: boolean;
  /** 可直接渲染的预览 URL（桌面的 asset URL）。缺失文件不给。 */
  previewUrl?: string;
  /** 需经 resolver 解析的取图源（Web 的 OPFS 路径、移动的 `file://`）。缺失文件不给。 */
  previewSource?: string;
}

/* ─── 适配器 ─── */

/** 发送侧已选文件。尚未进入任何会话，所以状态恒为 `idle`。 */
export function fromSelectedFiles(
  files: readonly SelectedFileInput[],
): FileBrowserItem[] {
  return files.map((file) => ({
    id: selectedFileId(file.sourceId),
    sourceId: file.sourceId,
    name: file.name,
    relativePath: normalizeRelativePath(file.relativePath, file.name),
    size: file.size,
    status: "idle" as const,
    ...(file.previewSource ? { previewSource: file.previewSource } : {}),
  }));
}

/**
 * 入站 offer 的文件清单。
 *
 * **状态是 `idle` 而不是 `waiting`**：offer 尚未进入传输队列，这份清单是「要不要收」
 * 的决策依据、一份只读的内容预览。给每一行挂上等待图标会暗示传输已经开始，
 * 而此刻用户连接受都还没点。
 *
 * **目录条目被丢掉**：offer 里的目录只是路径信息的载体，树形层级由
 * `buildFileBrowserTree` 从文件的 `relativePath` 自己派生。把目录也塞进 items 会让
 * 同一个目录出现两次（一次是 offer 给的条目，一次是建树时派生的）。
 */
export function fromOfferFiles(
  sessionId: string,
  files: readonly OfferFileInput[],
): FileBrowserItem[] {
  return files
    .filter((file) => !file.isDirectory)
    .map((file) => ({
      id: sessionFileId(sessionId, file.fileId),
      fileId: file.fileId,
      name: file.name,
      relativePath: normalizeRelativePath(file.relativePath ?? file.name, file.name),
      size: file.size,
      status: "idle" as const,
    }));
}

/**
 * 传输投影 + 实时进度 → 逐文件行。**本包最核心的一个函数。**
 *
 * 两条不变量：
 *
 * 1. **`files` 是骨架，`progress` 只是覆盖层。** 条目的身份、数量、名称、大小、路径**永远**
 *    来自投影；进度事件只按 `fileId` 覆盖「传了多少」与「什么状态」。二者不是二选一——
 *    Web 端此前写作 `live?.files ?? projection.files`，于是同一份数据有两种形状，
 *    消费点得靠 `"transferred" in file` 现场嗅探，切换会话时列表还会串。
 *
 * 2. **终态忽略进度。** 判定收在这里，不交给调用方自觉携带。`progress` 域在三端都是
 *    「只增不清」的：会话结束后那条进度事件仍留在 store 里，任何一个漏带判定的消费点
 *    都会读到陈旧快照。
 */
export function fromProjectionFiles(
  sessionId: string,
  files: readonly ProjectionFileInput[],
  options: {
    phase: SessionPhase;
    terminalReason?: SessionTerminalReason | null;
    progress?: readonly ProgressFileInput[] | null;
  },
): FileBrowserItem[] {
  const { phase, terminalReason, progress } = options;
  // 不变量 2：终态直接不查进度表。
  const liveByFileId =
    phase === "terminal"
      ? new Map<number, ProgressFileInput>()
      : new Map((progress ?? []).map((file) => [file.fileId, file]));

  return files.map((file) => {
    const live = liveByFileId.get(file.fileId);
    const transferred = live?.transferred ?? file.transferredBytes;
    const status = projectionFileStatus({
      phase,
      terminalReason,
      size: file.size,
      transferred,
      liveStatus: live?.status,
    });
    return {
      id: sessionFileId(sessionId, file.fileId),
      fileId: file.fileId,
      name: file.name,
      relativePath: normalizeRelativePath(file.relativePath, file.name),
      size: file.size,
      status,
      // 进度条只在「传了一半」的两档画。已完成的满格条把对勾又说了一遍，
      // 还没开始的空轨道则是一整行零信息——一次几十个文件的会话里那是几十条空轨道。
      ...(status === "transferring" || status === "paused"
        ? { progress: calcPercent(transferred, file.size) }
        : {}),
    };
  });
}

/**
 * 收件箱条目的文件行。已落盘，所以只有「在」与「不在」两种状态。
 *
 * **`sourceId` 与 `fileId` 是两个不同的号，别合并。** 前者是收件箱文件行的主键，
 * 「打开 / 在文件夹中显示」这类宿主动作按它定位（桌面的 `open_inbox_item(itemId, fileId)`
 * 里那个 `fileId` 查的就是 `detail.files[].id`）；后者是传输侧的文件号，用于跨会话对齐。
 * 两者数值上通常不同——把 `transferFileId` 传给宿主动作会打开另一个文件，或直接找不到。
 */
export function fromInboxFiles(
  itemId: string,
  files: readonly InboxFileInput[],
): FileBrowserItem[] {
  return files.map((file) => ({
    id: inboxFileId(itemId, file.id),
    sourceId: String(file.id),
    fileId: file.transferFileId ?? file.id,
    name: file.name,
    relativePath: normalizeRelativePath(file.relativePath, file.name),
    size: file.size,
    status: (file.missing ? "missing" : "completed") as FileBrowserStatus,
    // 缺失的文件两条取图路径都不给——它已经不在盘上，取图只会白白失败一次
    // （桌面那条更明显：`<img>` 会真去请求一次 asset URL 再落到 onError）。
    ...(file.missing ? {} : previewFields(file)),
  }));
}

function previewFields(
  file: InboxFileInput,
): Pick<FileBrowserItem, "previewUrl" | "previewSource"> {
  return {
    ...(file.previewUrl ? { previewUrl: file.previewUrl } : {}),
    ...(file.previewSource ? { previewSource: file.previewSource } : {}),
  };
}

/* ─── 内部 ─── */

/**
 * 单个文件的状态判定。
 *
 * 顺序有讲究：**内核给的逐文件状态优先**（它最准），其次是「字节数已满」，最后才按会话
 * 阶段推断。反过来的话，一个已经传完的文件会在会话仍是 `active` 时被判成 `transferring`。
 */
function projectionFileStatus(input: {
  phase: SessionPhase;
  terminalReason?: SessionTerminalReason | null;
  size: number;
  transferred: number;
  liveStatus?: ProgressFileStatus;
}): FileBrowserStatus {
  const { phase, terminalReason, size, transferred, liveStatus } = input;

  if (liveStatus === "completed") return "completed";
  if (liveStatus === "transferring") return "transferring";

  if (size > 0 && transferred >= size) return "completed";

  switch (phase) {
    case "active":
      return transferred > 0 ? "transferring" : "waiting";
    case "suspended":
      return transferred > 0 ? "paused" : "waiting";
    case "terminal":
      if (terminalReason === "completed") return "completed";
      // 过期与拒绝跟取消同档：都是「没传成，但没出错」。归到 error 会让它们在列表里
      // 与真正的传输故障混成一片。
      if (
        terminalReason === "cancelled" ||
        terminalReason === "rejected" ||
        terminalReason === "expired"
      ) {
        return "cancelled";
      }
      // `fatal_error` 与「内核将来新增的终态原因」都落这里——保守归类：宁可把没见过的
      // 原因显示成「出错」，也不要漏判成「已完成」。
      return "error";
    case "offered":
    case "waiting_accept":
      return "waiting";
  }
}
