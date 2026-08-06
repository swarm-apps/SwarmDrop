/**
 * 媒体类型判定 —— **已上移到 `@swarmdrop/shared-view`**，本模块只作转发。
 *
 * 上移前它是「file-browser 全链路唯一来源」，但那个「唯一」只在移动端成立：桌面另有一份
 * `PREVIEWABLE_IMAGE_EXTENSIONS`（多一个 `ico`、少 tiff），Web 一份都没有。三端共用之后
 * 取的是并集，并多出一个 `fileCategory`（图标分组用）。
 *
 * 保留本模块路径是为了不动本目录里的调用点。
 */

export {
  ARCHIVE_EXTENSIONS,
  AUDIO_EXTENSIONS,
  CODE_EXTENSIONS,
  DOCUMENT_EXTENSIONS,
  type FileCategory,
  fileCategory,
  IMAGE_EXTENSIONS,
  isImageFile,
  isVideoFile,
  VIDEO_EXTENSIONS,
} from "@swarmdrop/shared-view";
