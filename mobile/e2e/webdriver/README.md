# SwarmDrop-RN WebDriver E2E

这套 E2E 用 WebdriverIO + Appium XCUITest 驱动 iOS 端，避开 Maestro 在 iOS 26 / Expo SDK 56 / RN 0.85 Fabric 下 selector 不稳定的问题。

录制移动端 demo 时也使用当前 WebDriver 会话的 Appium 录屏接口，不要同时启动 Maestro：

```ts
await browser.startRecordingScreen();
// WebDriver 操作真实流程
const video = await browser.stopRecordingScreen();
```

`video` 是 base64 编码的 MP4。iOS 录屏不包含音频，并要求 Appium 主机安装 `ffmpeg`。双端录制脚本会
通过 `SWARMDROP_MOBILE_RECORDING_PATH` 把视频写入桌面仓库的 `build/desktop-recordings/raw/`。WDA 的
MJPEG 流使用 `10086` 端口。
如果该端口已被其他本地服务占用，可以临时设置 `SWARMDROP_MJPEG_SERVER_PORT=<空闲端口>` 覆盖。
Simulator 默认不启动 WDA MJPEG，使用 `simctl` 的 `--display=1` 录制，可通过
`SWARMDROP_IOS_DISPLAY` 覆盖。真实设备使用 Appium MJPEG 时设置
`SWARMDROP_APPIUM_SCREEN_RECORDING=1`。

## 准备

先启动一个 iOS Simulator，并安装 dev build：

```bash
pnpm exec expo run:ios --device "iPhone 17 Pro" --no-bundler
```

如果要用已经安装好的 app，直接跑：

```bash
pnpm e2e:ios
```

如果要让 Appium 安装指定 `.app`：

```bash
SWARMDROP_IOS_APP_PATH=/path/to/SwarmDrop.app pnpm e2e:ios
```

常用环境变量：

```bash
SWARMDROP_IOS_UDID=<simulator-or-device-udid>
SWARMDROP_IOS_DEVICE_NAME="iPhone 17 Pro"
SWARMDROP_IOS_PLATFORM_VERSION=26.4
SWARMDROP_IOS_NO_RESET=1
APPIUM_PORT=4723
```

## 选择器约定

移动端 WebDriver 统一使用 Accessibility ID：

```ts
await $("~onboarding-start-button").click();
```

React Native 侧要给关键交互补 `testID`，必要时同步 `accessibilityLabel`。iOS 上 `testID` 会映射到 accessibility identifier，是这条链路的稳定锚点。

## 注意

`appium driver install xcuitest` 在本仓库会被 `workspace:*` 依赖绊住，因为它内部调用 npm 修改当前包。这里改为通过 pnpm 安装 `appium-xcuitest-driver`，Appium 3 可以从项目依赖中识别 driver。
