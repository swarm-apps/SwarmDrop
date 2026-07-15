export type ApkProgressCallback = (downloaded: number, total: number) => void;

/** 投递的期望值,由 adapter 从 `ReleaseInfo` 取出传给下载器(只有 adapter 手上有它)。 */
export interface ApkDownloadExpectation {
  /** 期望字节数(来自 update 响应的 `size_bytes`);缺省则下载器跳过尺寸校验。 */
  sizeBytes?: number;
}

/**
 * APK 下载器(注入式)。把 `url` 下到本地缓存,边下边回调进度,
 * resolve 出本地 APK 路径(供 installer 消费)。
 *
 * **契约:下载器的职责是产出一个可用的 APK,而不是产出一个文件。** resolve 之前必须确认
 * 拿到的确实是 APK —— `createDownloadResumable` 对非 2xx **不抛错**,它把错误响应体(如
 * OSS 的 XML 错误页)照常写进目标文件并正常 resolve。校验归下载器所有(它才碰 expo-*,
 * adapter 因此得以保持纯逻辑可单测),而 adapter 的多源 failover **只在下载器抛错时才
 * 触发** —— 所以一个不校验的实现会让 failover 对它本该扛住的故障静默失效。
 *
 * 自行注入实现时:要么直接用 expo-downloader.ts 的 `createExpoApkDownloader`,要么照它的
 * `assertApkDownload` 自行校验(状态 + 非空 + 尺寸 + ZIP magic,失败先删文件再抛)。
 */
export interface ApkDownloader {
  /**
   * 下载 `url` 到本地;`onProgress(downloaded,total)` 报进度;resolve 本地 APK 路径。
   * `expected` 可选(既有两参实现仍可赋值);拿到则据以校验投递结果。
   */
  download(
    url: string,
    onProgress: ApkProgressCallback,
    expected?: ApkDownloadExpectation,
  ): Promise<string>;
}

/**
 * APK 安装器(注入式)。把本地 APK 交给系统安装器。
 *
 * **fire-and-forget handoff 语义**:install() 在安装 intent 派发后即 resolve —— 控制权
 * 已交给系统「安装新版本?」对话框,本 Promise **不**等待安装真正完成(Android 不允许
 * 第三方静默安装;用户确认后本进程会被替换,用户取消则下次 check 再弹)。
 */
export interface ApkInstaller {
  /** 把本地 `apkPath` 交给系统 PackageInstaller;intent 派发即 resolve。 */
  install(apkPath: string): Promise<void>;
}
