/**
 * 收件箱的**目录取回**：把一棵子树（或整条记录）打成一个 zip 再交给浏览器下载。
 *
 * ## 为什么不能「逐个文件下载」
 *
 * 浏览器上「取回一个目录」没有原生对应物，看起来最省事的做法是循环触发 N 次
 * `<a download>`。两条都过不去：
 *
 * 1. **连续多次程序化下载会被拦。** 浏览器对同一页面的第二次起下载弹一次「是否允许下载
 *    多个文件」，而我们的 `anchor.click()` 跑在 `await open_file()` 之后、早已脱离用户手势；
 *    被拦时**既不回调也不抛错**，落盘的只有最前面几个。而收件箱的文件顺序是
 *    `file_id` 升序 = 发送侧 walkdir 顺序（先第一层、再逐层深入），所以被吃掉的恰好是
 *    深层那些——线上症状正是「目录只下载了第一层」。
 * 2. **`<a download>` 留不住层级。** `download` 属性的值会被浏览器消毒掉路径分隔符，
 *    塞完整相对路径也没用，所有文件平铺进下载目录，同名的被改成 `a(1).txt`。
 *
 * zip 把两条一起解决：一次下载（不触发拦截）、路径写在 zip 条目名里（层级完整）。
 *
 * ## 已知限制
 *
 * `downloadZip(...).blob()` 会把整个 zip **物化**成一个 Blob（浏览器会视大小落到磁盘
 * backing store，但仍受存储配额约束）。真正的零内存写法是 File System Access 的
 * `showSaveFilePicker()` + `makeZip()` 流式写盘，但它只有 Chromium 有——那是后续增强，
 * 不是本模块今天要做的事。
 *
 * 本模块**不含任何 UI 文案**（`fallbackName` 由调用方翻译好传进来）：`_lib/` 下的纯函数
 * 不展开翻译宏，见知识库 web-app-frontend.md。
 */

import { downloadZip } from "client-zip";
import {
  normalizeDirectoryPath,
  normalizeRelativePath,
} from "@swarmdrop/shared-view";

/** 一条打包计划项：从哪儿读、在 zip 里叫什么。 */
export interface ZipEntryPlan {
  /** OPFS 键，也就是 `WebNode.open_file()` 的入参（收件箱行的 `relativePath`）。 */
  sourcePath: string;
  /** zip 内的路径（带层级）。 */
  entryName: string;
}

export interface ZipPlan {
  /** 下载下来的 zip 叫什么（含 `.zip` 后缀）。 */
  fileName: string;
  entries: ZipEntryPlan[];
  /**
   * 这些条目的原始字节数之和（zip 走 store 不压缩，所以它约等于成包大小）。
   *
   * 存在的理由是**整包会被一次性物化**（见模块注释的「已知限制」）：调用方据此判断
   * 要不要先提醒用户，而不是让一次 5 GB 的下载在读完所有 OPFS 字节之后才崩掉。
   */
  totalSize: number;
}

/** 打包计划的输入：只声明用到的三个字段，与收件箱行 DTO 解耦。 */
export interface ZipSourceFile {
  relativePath: string;
  name: string;
  size: number;
}

/**
 * 把一个目录打成 zip 的计划。
 *
 * **zip 内保留目录名自身那一层**（`photos/2024/` → 条目是 `2024/a.jpg`），解压出来就是一个
 * `2024/` 文件夹而不是散落一地的文件——与各家云盘「下载文件夹」的结果一致。
 */
export function planDirectoryZip(
  files: readonly ZipSourceFile[],
  relativeDirectory: string,
): ZipPlan {
  const directory = normalizeDirectoryPath(relativeDirectory);
  // 相对于**父目录**切，切口就落在目录名之前——这是「保留自身那一层」的全部实现。
  // 归一后没有空段也没有尾斜杠，所以最后一个 `/` 之后就是目录名；顶层目录没有 `/`，
  // `-1 + 1 = 0` 正好表示「不切」。
  const parentPrefixLength = directory.lastIndexOf("/") + 1;
  const folderName = directory.slice(parentPrefixLength);
  const entries: ZipEntryPlan[] = [];
  let totalSize = 0;
  // 空目录前缀会匹配上所有文件（同 `isPathInsideDirectory` 的约定）——而调用点是
  // 「把这个目录取回来」，宁可取回零个也不能取回全部。
  if (!directory) {
    return { fileName: zipFileName(folderName), entries, totalSize };
  }

  for (const file of files) {
    // 一趟循环里判归属 + 切路径，每个文件只归一**一次**。
    // 不用 `isPathInsideDirectory`：它内部会把两个入参都再归一一遍，而 `directory`
    // 上面已经归一过了——几百文件的条目上那就是几百次白跑的 split/filter/join。
    // 判据本身照抄它（按 segment 边界比，`a/b` 不会误匹配 `a/bc`）。
    const path = normalizeRelativePath(file.relativePath, file.name);
    if (path !== directory && !path.startsWith(`${directory}/`)) continue;
    entries.push({
      sourcePath: file.relativePath,
      entryName: path.slice(parentPrefixLength),
    });
    totalSize += file.size;
  }

  return { fileName: zipFileName(folderName), entries, totalSize };
}

/**
 * 把整条记录打成 zip 的计划。
 *
 * 全部文件同处一个顶层目录时用那个目录名（对方发来一整个文件夹，这是最常见的形态），
 * 否则回落到调用方给的名字——散文件没有「那个目录」可言，拿首文件名去命名一个装着
 * 五个文件的压缩包只会误导。
 */
export function planItemZip(
  files: readonly ZipSourceFile[],
  fallbackName: string,
): ZipPlan {
  const entries = files.map((file) => ({
    sourcePath: file.relativePath,
    entryName: normalizeRelativePath(file.relativePath, file.name),
  }));
  return {
    fileName: zipFileName(commonTopDirectory(entries) || fallbackName),
    entries,
    totalSize: files.reduce((sum, file) => sum + file.size, 0),
  };
}

/**
 * 全部条目共有的顶层目录段；不存在（有文件直接躺在根上，或分属多个顶层目录）时返回空串。
 *
 * 收 entries 而不是先 `map` 出一份路径数组：那份数组用一次就扔，而这里只是要数它有几种
 * 首段。
 */
function commonTopDirectory(entries: readonly ZipEntryPlan[]): string {
  // 没有 `/` 的路径（文件直接躺在根上）取到空串，于是它必然与任何真实目录名不等 ——
  // 集合里一旦多出这个空串，`size === 1` 就不成立，「有文件在根上」自动落到回落分支。
  const tops = new Set(
    entries.map(({ entryName }) =>
      entryName.slice(0, Math.max(entryName.indexOf("/"), 0)),
    ),
  );
  const [only] = tops;
  return tops.size === 1 ? (only ?? "") : "";
}

/**
 * 拼 zip 文件名。
 *
 * 路径分隔符要挡掉——`<a download>` 会把它消毒成别的字符，用户看到的文件名会变成一串
 * 意料之外的东西。空名回落 `download`，绝不产出一个叫 `.zip` 的隐藏文件。
 */
function zipFileName(base: string): string {
  const safe = base.replace(/[/\\]/g, "_").trim();
  return `${safe || "download"}.zip`;
}

export interface ZipResult {
  blob: Blob;
  /** 取不到字节因而没进包的 OPFS 键。 */
  skipped: string[];
}

/**
 * 「别再往下打了」——取字节的那一方用它中止整次打包，而不是被算作又一个跳过的文件。
 *
 * **两件事必须分开**：单个条目取不到（OPFS 驱逐）是常态，跳过它继续打是对的；而
 * 「节点已经被用户停掉」意味着**后面每一个都会失败**，继续下去只会产出一个静默截断的
 * 压缩包，外加一句归因完全错误的提示（「这些文件已不在浏览器存储中」——其实文件好好的）。
 */
export class ZipAbortedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ZipAbortedError";
  }
}

/** 打包过程中最多打几条「这个文件取不到」的诊断。 */
const MAX_SKIP_LOGS = 5;

/**
 * 超过这个体积就先提醒用户「可能失败」。
 *
 * 判据来自失败模式而不是某个规范上限：`downloadZip(...).blob()` 把整包一次性物化，
 * Chrome 的 Blob 后备存储会溢写磁盘（仍占存储配额），Firefox / Safari 基本驻内存。
 * 1 GiB 是「开始有理由担心」的量级，而**失败发生在读完所有 OPFS 字节之后**——
 * 用户会白等很久才看到崩溃，所以提醒必须在开工之前给。
 *
 * 它只提醒、不拦截：拦了就等于把「大目录取不回来」变成产品行为，而多数情况下它是能成的。
 */
export const ZIP_SIZE_WARN_BYTES = 1024 ** 3;

/**
 * 按计划取字节并打包。
 *
 * **取不到的条目跳过而不是让整包失败**：OPFS 是配额存储，条目可能被浏览器驱逐，
 * 「一个死路径 → 整次下载失败」正是 `send_inbox_files` 那条既有约定要杜绝的。
 * 跳过了哪些由调用方决定怎么说（返回值里带着）。**例外是 [`ZipAbortedError`]**，
 * 它原样抛出去。
 *
 * 用 async generator 而不是先 `Promise.all` 拿齐所有 `File`：client-zip 边取边写，
 * 同一时刻只有一个 OPFS 句柄在解析（`open_file` 带 5 秒超时兜底，一批未就绪的路径
 * 并发打进去会一起挂在那里）。
 */
export async function buildZip(
  plan: ZipPlan,
  openFile: (path: string) => Promise<File>,
  lastModified: Date,
): Promise<ZipResult> {
  const skipped: string[] = [];

  async function* entries() {
    for (const entry of plan.entries) {
      let file: File;
      try {
        file = await openFile(entry.sourcePath);
      } catch (e) {
        if (e instanceof ZipAbortedError) throw e;
        // 控制台留一条带路径的诊断：界面上只报得下「跳过了 N 个」。整条记录被驱逐时
        // 这里会是几百条同样的话，把真正要看的第一条冲掉——所以只打前几条，
        // 完整清单由 `skipped` 带回给调用方。
        if (skipped.length < MAX_SKIP_LOGS) {
          console.error(`[web] 打包跳过取不到的文件 ${entry.sourcePath}`, e);
        }
        skipped.push(entry.sourcePath);
        continue;
      }
      yield { name: entry.entryName, input: file, lastModified };
    }
    if (skipped.length > MAX_SKIP_LOGS) {
      console.error(`[web] 打包共跳过 ${skipped.length} 个取不到的文件`);
    }
  }

  const blob = await downloadZip(entries()).blob();
  return { blob, skipped };
}
