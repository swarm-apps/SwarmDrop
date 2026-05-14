/**
 * file-picker
 * 桌面端文件系统操作封装（移动端已迁移到 RN，使用 expo-file-system）
 */

import { open } from "@tauri-apps/plugin-dialog";
import { downloadDir, join } from "@tauri-apps/api/path";
import type { FileSource, SaveLocation } from "@/commands/transfer";

const SAVE_DIR_NAME = "SwarmDrop";

/**
 * 获取默认保存路径
 */
export async function getDefaultSavePath(): Promise<string> {
  const dir = await downloadDir();
  return join(dir, SAVE_DIR_NAME);
}

/**
 * 选择文件
 * @param multiple 是否允许多选
 */
export async function pickFiles(multiple = true): Promise<FileSource[]> {
  const selected = await open({ multiple });
  if (!selected) return [];
  const paths = Array.isArray(selected) ? selected : [selected];
  return paths.map((p) => ({ type: "path" as const, path: p }));
}

/**
 * 选择文件夹
 */
export async function pickFolder(
  defaultPath?: string,
): Promise<string | null> {
  return await open({ directory: true, defaultPath });
}

/**
 * 打开文件夹（在系统文件管理器中显示）
 */
export async function openFolder(path: string): Promise<boolean> {
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
  return true;
}

/**
 * 用系统默认应用打开文件
 */
export async function openFile(path: string): Promise<void> {
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}

/**
 * 在文件管理器中显示并选中文件
 */
export async function revealFile(filePath: string): Promise<void> {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(filePath);
}

/**
 * 打开传输完成后的文件/文件夹
 */
export async function openTransferResult(session: {
  saveLocation?: SaveLocation;
  files: { relativePath: string }[];
}): Promise<void> {
  if (!session.saveLocation) return;

  const loc = session.saveLocation;
  if (loc.type !== "path") return;

  if (session.files.length === 1) {
    const filePath = await join(loc.path, session.files[0].relativePath);
    await revealFile(filePath);
  } else {
    await openFolder(loc.path);
  }
}

/**
 * 选择文件夹（用于发送）
 */
export async function pickFolderAsSource(): Promise<FileSource | null> {
  const path = await open({ directory: true });
  if (!path) return null;
  return { type: "path" as const, path };
}
