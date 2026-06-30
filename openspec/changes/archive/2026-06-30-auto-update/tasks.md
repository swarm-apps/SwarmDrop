## 1. 依赖与配置

- [x] 1.1 Rust 端添加 `tauri-plugin-updater` 和 `tauri-plugin-process` 依赖到 `src-tauri/Cargo.toml`
- [x] 1.2 前端添加 `@tauri-apps/plugin-updater` 和 `@tauri-apps/plugin-process` 依赖
- [x] 1.3 在 `src-tauri/src/lib.rs` 中注册 updater 和 process 插件（参考 cc-switch 的容错注册方式）
- [x] 1.4 配置 `src-tauri/tauri.conf.json`：添加 `bundle.createUpdaterArtifacts: true`、`plugins.updater`（pubkey、endpoints）
- [x] 1.5 在 `src-tauri/capabilities/default.json` 添加 `updater:default` 和 `process:allow-restart` 权限
- [x] 1.6 生成 minisign 密钥对，公钥写入 tauri.conf.json，私钥记录待配置到 GitHub Secrets（⚠️ 需手动执行 `cargo tauri signer generate -w keys/swarmdrop.key`）

## 2. 更新状态管理

- [x] 2.1 创建 `src/stores/update-store.ts`，定义 UpdateStatus 类型和 store（status、version、progress、error 等字段）
- [x] 2.2 实现 `checkForUpdate()` action：桌面端调用 tauri-plugin-updater API，移动端 fetch latest.json
- [x] 2.3 实现 `downloadAndInstall()` action（桌面端）：下载 + 进度回调 + 安装重启
- [x] 2.4 实现 `openDownloadPage()` action（移动端）：调用 tauri-plugin-opener 打开浏览器
- [x] 2.5 实现 min_version 比较逻辑（semver 比较），判断是否需要强制更新

## 3. 前端更新封装

- [x] 3.1 创建 `src/commands/updater.ts`：封装桌面端 updater API（check、download、install）
- [x] 3.2 创建 `src/lib/version.ts`：semver 比较工具函数 + latest.json 移动端解析
- [x] 3.3 实现启动自动检查逻辑：在 `_app.tsx` 或 App 入口延迟 3 秒调用 checkForUpdate

## 4. 设置页更新 UI

- [x] 4.1 创建 `src/components/settings/AboutSection.tsx`：关于区域组件（应用信息 + 更新按钮 + 版本描述）
- [x] 4.2 实现「无更新」状态 UI：显示「检查更新」按钮 + 「更新日志」按钮
- [x] 4.3 实现「有更新」状态 UI：蓝色 banner + 「更新到 vX.X.X」按钮（桌面）/ 「前往下载」按钮（移动）
- [x] 4.4 实现「下载中」状态 UI（桌面端）：进度条 + 百分比 + 已下载/总大小 + 速度 + 「下载中...」禁用按钮
- [x] 4.5 将 AboutSection 集成到桌面端和移动端设置页路由

## 5. 强制更新弹窗

- [x] 5.1 创建 `src/components/ForceUpdateDialog.tsx`：桌面端强制更新弹窗（不可关闭模态框）
- [x] 5.2 实现强制更新弹窗内容：红色警告图标、版本信息卡片（当前/最低/最新）、黄色警告条、「立即更新」按钮
- [x] 5.3 实现强制更新下载进度状态：蓝色下载图标、进度条、速度统计、底部提示
- [x] 5.4 创建移动端强制更新弹窗变体：「前往下载」按钮 + external-link 图标
- [x] 5.5 在应用根层级（_app.tsx 或 __root.tsx）集成强制更新弹窗，当 status 为 `force-required` 时渲染

## 6. 国际化

- [x] 6.1 为所有更新相关文案添加 Lingui 翻译标记（t`` / Trans）
- [x] 6.2 运行 `pnpm i18n:extract` 提取新翻译字符串
- [x] 6.3 翻译 8 个语言的更新相关条目（zh、zh-TW、en、ja、ko、es、fr、de）

## 7. CI/CD

- [x] 7.1 创建 `.github/workflows/release.yml`：多平台构建 + 签名工作流
- [x] 7.2 实现 `assemble-latest-json` job：从各平台构建产物组装 latest.json，追加 mobile.android 字段
- [x] 7.3 配置 GitHub Secrets：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- [x] 7.4 测试完整的 release → latest.json → 客户端检测 链路
