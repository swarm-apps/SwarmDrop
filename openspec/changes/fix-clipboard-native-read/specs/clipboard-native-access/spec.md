# clipboard-native-access

## ADDED Requirements

### Requirement: 桌面剪贴板读写经原生 plugin

桌面前端读写系统剪贴板 SHALL 一律经 `tauri-plugin-clipboard-manager`，并收口在
`src/lib/clipboard.ts` 单一封装模块。`src-tauri/capabilities/default.json` SHALL 同时授予
`clipboard-manager:allow-write-text` 与 `clipboard-manager:allow-read-text`。

桌面前端代码 SHALL NOT 直接使用 `navigator.clipboard`（WebView API 在桌面壳里会触发权限
申请弹窗，且各平台 WebView 行为不一致）。

#### Scenario: 读取剪贴板中的邀请

- **WHEN** 用户复制邀请后切回应用窗口，剪贴板感知触发一次读取
- **THEN** 读取经原生 plugin 完成，不弹出任何浏览器权限申请弹窗

#### Scenario: 读取失败不静默无痕

- **WHEN** 剪贴板读取失败（剪贴板为空、内容非文本、权限缺失 —— 插件底层是 arboard 的
  `ContentNotAvailable`，这几种原因**在 API 层不可区分**）
- **THEN** 应用静默降级（不弹用户可见错误，剪贴板感知只是增强路径），但记录一条调试日志，
  使「该功能在某平台从未生效」可被发现；调用方不得据错误推断具体原因

### Requirement: WebView 剪贴板 API 的机器兜底

仓库 SHALL 提供检查脚本，扫描桌面前端源码中对 `navigator.clipboard` 的直接引用并在命中时
失败退出，接入 `package.json` 的 script 供本地与 CI 调用。

#### Scenario: 新代码直接用了 WebView 剪贴板 API

- **WHEN** 有人在 `src/` 下新写 `navigator.clipboard.readText()` 或 `writeText()`
- **THEN** 检查脚本失败并指出文件与行号，提示改用 `src/lib/clipboard.ts` 的封装
