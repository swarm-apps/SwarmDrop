import { i18n } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { openSaveFolder } from "@/core/saf-intent";
import { toast } from "@/lib/toast";

/**
 * 「打开保存目录,失败 toast 提示」—— 传输详情 / 收件箱详情共用的 UI 层薄封装,
 * 两页不用各自重复 try/catch + 同一句文案。
 */
export async function openSaveFolderOrToast(uri: string): Promise<void> {
  try {
    await openSaveFolder(uri);
  } catch {
    toast.error(i18n._(msg`无法打开文件夹`));
  }
}
