// rn registry 的注入式端口 —— rn-adapter.ts 只依赖这两个接口 + @swarm-hive/sdk,
// 让 adapter 本体可纯逻辑单测(不碰 expo-* 真实实现)。真实实现见同目录
// expo-downloader.ts / expo-installer.ts,由用户在装配时注入。
//
// 语义对齐 SwarmHive SDK 的 UpdateAdapter:
// - download 把远端 URL 落到本地,产出一个本地 APK 路径(string);
// - install 是 fire-and-forget handoff —— 把 APK 交给系统 PackageInstaller(via
//   ACTION_VIEW intent),intent 派发即 resolve(SDK engine 无 installing 态)。

/** 下载进度回调:累计已下载字节 + 期望总字节(任一为 0/未知时由 adapter 兜底 percent)。 */
export type ApkProgressCallback = (downloaded: number, total: number) => void;

/**
 * APK 下载器(注入式)。把 `url` 下到本地缓存,边下边回调进度,
 * resolve 出本地文件路径(供 installer 消费)。
 */
export interface ApkDownloader {
  /** 下载 `url` 到本地;`onProgress(downloaded,total)` 报进度;resolve 本地 APK 路径。 */
  download(url: string, onProgress: ApkProgressCallback): Promise<string>;
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
