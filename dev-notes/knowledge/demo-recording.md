# SwarmDrop 演示录制

桌面端和移动端的素材录制是产品演示资产，不与普通 E2E 混用。录制目标是得到可直接用于文档或官网的纯应用画面，并且传输类演示必须等发送端和接收端都进入成功状态。

## 录制入口

### 桌面端

桌面端使用 WDIO 驱动 Tauri、OBS 采集原生窗口：

```bash
# 单次启动应用，连续录制首页、发送入口和收件箱
pnpm --dir e2e/desktop record:desktop-suite

# 桌面到 iOS 的完整真实传输编排
pnpm --dir e2e/desktop record:transfer
```

`desktop-suite` 在一个 WDIO 会话中执行全部基础场景，避免每段素材重复启动和关闭应用。`record:transfer` 只有在桌面端的 `send-success-state` 和移动端的 `transfer-success-state` 都出现后才算完成。

### Android 虚拟机

Android 使用系统 `adb screenrecord`，已在本机 `Pixel_7` AVD 实测可用：

```bash
# 手动开始，按 Ctrl+C 停止
pnpm --dir e2e/desktop record:mobile android

# 自动录制 10 秒
pnpm --dir e2e/desktop record:mobile android 10
```

脚本会把模拟器内的临时视频自动拉取到 `e2e/desktop/build/desktop-recordings/raw/`。2026-07-14 的验证产物为 H.264、1080×2400、5.68 秒、102 个视频包。

### iOS 模拟器

本机 Xcode 26.4.1 下，命令行 `xcrun simctl io ... recordVideo` 即使在关闭并冷启动 Simulator 后仍会报 `SimRenderServer.SimulatorError Code=2`。这是本机 CoreSimulator 图形录制链路的问题，不是 SwarmDrop 的问题；不要把该命令作为当前环境的可用录制入口。

Simulator 自带的 `Cmd+R` 交互式录制已由人工确认可用。录制 iOS 模拟器素材时：

1. 用 WebDriver 或手动操作把应用停在待演示状态。
2. 在 Simulator 中按 `Cmd+R` 开始录制。
3. 完成操作后再次按 `Cmd+R` 停止并保存视频。

它是当前最简单可靠的 iOS Simulator 方案。若需要录制过程完全自动化，改用真实 iPhone：项目已有 Appium XCUITest 的 `startRecordingScreen` / `stopRecordingScreen` 路径，设置 `SWARMDROP_APPIUM_SCREEN_RECORDING=1` 后由 WDA 与 ffmpeg 采集纯设备画面。

## 产物约定

- 原始视频：`e2e/desktop/build/desktop-recordings/raw/`
- 录制 manifest：`e2e/desktop/build/desktop-recordings/manifests/`
- 关键截图：`e2e/desktop/build/wdio/screenshots/`

这些均是构建产物，不提交到 Git。
