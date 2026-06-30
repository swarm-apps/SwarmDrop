## ADDED Requirements

### Requirement: 移动端版本检测
系统 SHALL 在 Android 平台启动时通过 HTTP fetch 请求 latest.json，解析 `mobile.android` 字段获取最新版本信息。

#### Scenario: 检测到新版本
- **WHEN** 应用启动后 fetch latest.json 成功
- **THEN** 系统比较 `mobile.android.version` 与本地应用版本
- **THEN** 若远程版本高于本地版本，更新状态变为 `available`

#### Scenario: 已是最新版本
- **WHEN** fetch 成功且 `mobile.android.version` 等于本地版本
- **THEN** 更新状态变为 `up-to-date`

#### Scenario: 网络请求失败
- **WHEN** fetch latest.json 失败
- **THEN** 静默失败，不影响正常使用

### Requirement: 移动端引导下载
系统 SHALL 在检测到新版本时，提供「前往下载」按钮，点击后通过 tauri-plugin-opener 打开浏览器跳转到 APK 下载链接。

#### Scenario: 用户点击前往下载
- **WHEN** 用户在移动端设置页点击「前往下载」按钮
- **THEN** 系统调用 tauri-plugin-opener 在默认浏览器中打开 `mobile.android.download_url`

### Requirement: latest.json 移动端扩展
CI/CD SHALL 在 latest.json 中生成 `mobile.android` 字段，包含 `version`、`download_url`、`min_version`。

#### Scenario: latest.json 包含移动端信息
- **WHEN** CI/CD 构建并发布新版本
- **THEN** latest.json 包含 `mobile.android.version`（语义化版本号）
- **THEN** latest.json 包含 `mobile.android.download_url`（APK 直链）
- **THEN** latest.json 包含 `mobile.android.min_version`（最低兼容版本）
