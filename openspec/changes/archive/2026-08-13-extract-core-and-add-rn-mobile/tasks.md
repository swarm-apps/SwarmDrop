## 1. 工作区与边界准备

- [x] 1.1 盘点 `src-tauri/src` 中可复用业务模块、Tauri 专属模块、移动端暂不迁移模块，并记录迁移归属
- [x] 1.2 将 Rust 工程调整为工作区结构，新增 `crates/core`、`crates/entity`、`crates/migration` 的空壳
- [x] 1.3 调整 `src-tauri/Cargo.toml` 依赖，使桌面端通过路径依赖引用共享 crates
- [x] 1.4 确认 `libs/core` 子模块依赖路径在新工作区下仍可用于桌面与移动构建
- [x] 1.5 增加工作区级别的 `cargo check` 验证命令说明

## 2. 共享 Core 基础抽离

- [x] 2.1 将通用错误类型迁移到 `swarmdrop-core`，并保留 Tauri 命令可序列化的错误转换
- [x] 2.2 将 P2P 协议类型迁移到 `swarmdrop-core`
- [x] 2.3 将设备身份与设备信息模型迁移到 `swarmdrop-core`
- [x] 2.4 将配对分享码模型和校验逻辑迁移到 `swarmdrop-core`
- [x] 2.5 将数据库 entity 迁移到 `crates/entity`
- [x] 2.6 将数据库 migration 迁移到 `crates/migration`
- [x] 2.7 为 core 暴露稳定的公共模块入口，避免桌面端依赖内部路径

## 3. Host Traits 与运行时接口

- [x] 3.1 定义 `KeychainProvider` trait，用于保存、读取、删除设备身份和迁移状态
- [x] 3.2 定义 `EventBus` trait，用于向宿主转发网络、配对、传输和错误事件
- [x] 3.3 定义 `AppPaths` trait，用于提供数据库、缓存、临时文件和日志路径
- [x] 3.4 定义 `FileAccess` trait，用于抽象发送文件读取和接收文件写入
- [x] 3.5 定义 `Notifier` trait，用于抽象桌面通知和移动端通知
- [x] 3.6 定义 `UpdateInstaller` trait，并允许移动端提供 no-op 实现
- [x] 3.7 为所有 host trait 添加内存实现，供 core 单元测试使用

## 4. Core 业务迁移

- [x] 4.1 将网络节点启动、关闭和状态查询迁移为 core runtime API
- [x] 4.2 将 libp2p 事件循环迁移到 core，并通过 `EventBus` 分发事件
- [x] 4.3 将配对发布、查询、接受和拒绝流程迁移到 core
- [x] 4.4 将已配对设备持久化逻辑迁移到 core
- [x] 4.5 将设备列表和网络状态聚合逻辑迁移到 core
- [x] 4.6 将文件传输请求、进度事件和取消逻辑迁移到 core
- [x] 4.7 将发送文件读取入口改为 `FileAccess` source
- [x] 4.8 将接收文件保存入口改为 `FileAccess` sink
- [x] 4.9 为 core 添加内存 host 的网络、配对、身份、文件访问单元测试

## 5. 桌面端 Host 适配

- [x] 5.1 在 `src-tauri` 中实现桌面 `KeychainProvider`
- [x] 5.2 在 `src-tauri` 中实现桌面 `EventBus`，继续通过 Tauri channel 或事件转发给前端
- [x] 5.3 在 `src-tauri` 中实现桌面 `AppPaths`
- [x] 5.4 在 `src-tauri` 中实现桌面 `FileAccess`
- [x] 5.5 在 `src-tauri` 中实现桌面 `Notifier`
- [x] 5.6 在 `src-tauri` 中实现桌面 `UpdateInstaller`
- [x] 5.7 将现有 Tauri commands 改为调用 core runtime API
- [x] 5.8 保持 TypeScript `src/commands` 对上层页面的调用契约不变
- [x] 5.9 验证桌面端设备发现、配对、网络状态和文件传输流程仍可运行

## 6. 身份与免密码启动

- [x] 6.1 将设备身份初始化改为优先从 `KeychainProvider` 读取
- [x] 6.2 在首次启动时自动生成并保存设备身份，不要求用户设置启动密码
- [x] 6.3 废弃旧 Stronghold 身份数据，切换为新的 keychain identity 存储
- [x] 6.4 清理旧密码/Stronghold 启动路径，不保留兼容迁移分支
- [x] 6.5 首次启动或旧数据存在时直接生成/保存新的设备身份
- [x] 6.6 调整桌面认证路由，跳过每次启动输入密码的强制流程
- [x] 6.7 保留可选的本地锁定或生物识别设置入口
- [x] 6.8 验证 keychain identity 在重启初始化后 PeerId 保持稳定

## 7. React Native 项目脚手架

- [x] 7.1 在同级目录创建 `../swarmdrop-mobile`
- [x] 7.2 参考 `../swarmnote-mobile` 建立 Expo React Native 项目结构
- [x] 7.3 配置 `pnpm-workspace.yaml` 和包管理脚本
- [x] 7.4 配置 Expo Router、TypeScript、ESLint 和基础构建脚本
- [x] 7.5 增加 `packages/swarmdrop-core` RN 原生包目录
- [x] 7.6 配置 development build，明确不支持 Expo Go 运行
- [x] 7.7 添加 Android 和 iOS 原生工程生成或预构建说明

## 8. UniFFI 移动桥

- [x] 8.1 新增 UniFFI wrapper crate，并通过路径依赖引用 `swarmdrop-core`
- [x] 8.2 定义移动端需要的 UniFFI UDL/API 边界
- [x] 8.3 暴露 core 初始化、身份初始化、节点启动、节点关闭 API
- [x] 8.4 暴露配对发布、配对查询、接受配对、拒绝配对 API
- [x] 8.5 暴露设备列表、网络状态、传输状态查询 API
- [x] 8.6 暴露发送文件、接收确认、取消传输 API
- [x] 8.7 实现 RN 事件回调桥，将 core 事件转为 JS 可订阅事件
- [x] 8.8 配置 Android 构建产物生成和 RN 包装层
- [x] 8.9 配置 iOS 构建产物生成和 RN 包装层
- [x] 8.10 为桥接层添加基础 smoke test 或示例调用

## 9. 移动端 Host 适配

- [x] 9.1 使用移动端安全存储实现 RN `KeychainProvider`
- [x] 9.2 使用 RN 事件订阅实现移动端 `EventBus`
- [x] 9.3 使用移动端文档目录和缓存目录实现 `AppPaths`
- [x] 9.4 使用系统文件选择器和 URI 读取实现发送端 `FileAccess`
- [x] 9.5 使用保存面板或应用文档目录实现接收端 `FileAccess`
- [x] 9.6 为移动端通知实现 MVP 级 `Notifier`
- [x] 9.7 为移动端实现 no-op `UpdateInstaller`
- [x] 9.8 验证移动端前台运行时可以初始化 core 并启动节点

## 10. 移动端 MVP 界面与状态

- [x] 10.1 实现移动端首次启动引导页
- [x] 10.2 实现移动端设备身份初始化状态页
- [x] 10.3 实现移动端设备列表页
- [x] 10.4 实现移动端网络状态展示
- [x] 10.5 实现移动端发布分享码流程
- [x] 10.6 实现移动端输入分享码并发起配对流程
- [x] 10.7 实现移动端配对请求确认流程
- [x] 10.8 实现移动端选择文件并发送流程
- [x] 10.9 实现移动端接收文件确认和进度展示流程
- [x] 10.10 实现移动端错误、重试、空状态和权限拒绝状态

## 11. 跨端联调与验证

- [ ] 11.1 验证桌面端新版本首次启动不再强制输入密码
- [ ] 11.2 验证旧桌面 Stronghold 数据被废弃时会生成新的 keychain PeerId
- [ ] 11.3 验证桌面端与桌面端仍可配对和传输
- [ ] 11.4 验证移动端与桌面端可通过分享码完成配对
- [ ] 11.5 验证桌面端向移动端发送小文件成功
- [ ] 11.6 验证移动端向桌面端发送小文件成功
- [ ] 11.7 验证移动端前台切后台再返回后的网络状态恢复策略
- [x] 11.8 验证 Android 构建通过
- [ ] 11.9 验证 iOS 构建或记录本机环境缺口
- [x] 11.10 运行前端、Rust、RN 相关 lint/check/test 命令

## 12. 文档与收尾

- [x] 12.1 更新开发文档，说明 core/desktop/mobile 的新架构边界
- [x] 12.2 更新移动端开发命令和环境准备说明
- [x] 12.3 更新桌面端免密码启动和可选锁定行为说明
- [x] 12.4 标记 `keychain-based-identity` 变更已被本变更吸收或废弃
- [x] 12.5 清理迁移过程中遗留的 Tauri 专属业务依赖
- [x] 12.6 为后续后台传输、二维码配对、商店发布建立后续 OpenSpec 候选项
