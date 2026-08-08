/** 下载进度回调:累计已下载字节 + 期望总字节(任一为 0/未知时由 adapter 兜底 percent)。 */
export type ApkProgressCallback = (downloaded: number, total: number) => void;

/**
 * 一个候选产物的期望值,由 adapter 从 `ReleaseInfo` 取出(只有它手上有 release)。
 * `download` 用它校验投递并给落盘产物打标,`reconcile` 用它复检磁盘上的残留 ——
 * 同一组判据,所以是同一个类型。
 */
export interface ApkArtifactExpectation {
  /**
   * 该产物对应的版本标识(RN 用 versionCode 字符串)。下载器据它给落盘的产物打标,
   * 下次进程起来时才认得出「磁盘上这个包是给哪一版的」。
   */
  version: string;
  /** 期望字节数(来自 update 响应的 `size_bytes`);缺省则跳过尺寸校验。 */
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
   * `expected` 用于校验投递结果并给落盘产物打标。
   */
  download(
    url: string,
    onProgress: ApkProgressCallback,
    expected: ApkArtifactExpectation,
  ): Promise<string>;
  /**
   * 可选:把上个进程留下的产物与候选对齐,喂给 SDK 的 `UpdateAdapter.reconcile`。
   *
   * - `expected` 非空且磁盘产物匹配且完整 → resolve 本地 APK 路径(可直接安装);
   * - `expected` 非空但不匹配/损坏 → 清理产物,resolve null;
   * - `expected` 为 null → 清理产物,resolve null。
   *
   * 实现**必须**在 resolve null 时把不再有用的残留删掉 —— 否则装过的包会永久占着缓存。
   */
  reconcile?(expected: ApkArtifactExpectation | null): Promise<string | null>;
}

/**
 * `install` 被门禁拦下的原因。UI 据此选择引导文案,无需依赖任何 expo-* 符号。
 *
 * 目前只有一种。写成联合类型是为了将来加原因时不必改判别方式 —— 但**不要预先加**一个
 * 产生不出来的取值:那会让 UI 长出一条永远走不到的分支,并让人以为已经有对应的引导了。
 */
export type ApkInstallBlockReason =
  /** app 不在前台 —— Android 10+ 会静默丢弃后台派发的 Activity 启动。 */
  "background";

/**
 * APK 安装器(注入式)。把本地 APK 交给系统安装器。
 *
 * **fire-and-forget handoff 语义**:install() 在安装 intent 派发后即 resolve —— 控制权
 * 已交给系统「安装新版本?」对话框,本 Promise **不**等待安装真正完成(Android 不允许
 * 第三方静默安装;用户确认后本进程会被替换,用户取消则下次 check 再弹)。
 *
 * 正因为 resolve 不代表任何结果,**派发前的门禁是这一层唯一能给出的真实信号**:
 * app 不在前台时**返回** `{ reason: "background" }` 而不是发一个必然被系统丢弃的 intent。
 * 返回而非抛错 —— 那不是失败,什么都没发生过(见 SDK `UpdateAdapter.install` 的说明)。
 * 调用方(SDK engine)可以反复重试同一个句柄,门禁不消耗产物。
 *
 * 「未授权安装未知应用」**刻意不做门禁**:`expo-intent-launcher` 不暴露
 * `canRequestPackageInstalls`,内建的探测只能靠猜,而一个错误的「已拒绝」会挡掉本来能装的
 * 更新。未授权时照常派发,Android 自己会把用户领到授权页 —— 授权后返回,ready 还在,
 * 点「立即安装」即可。这是一条通路,不是死路。
 */
export interface ApkInstaller {
  /**
   * 把本地 `apkPath` 交给系统 PackageInstaller;intent 派发即 resolve。
   * 前置门禁未过时返回 `{ reason }`(未派发任何 intent)。
   */
  install(apkPath: string): Promise<void | { reason: ApkInstallBlockReason }>;
}
