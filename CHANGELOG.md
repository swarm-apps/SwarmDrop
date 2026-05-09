# Changelog

All notable changes to SwarmDrop will be documented in this file.

## [0.4.4](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.4.4) - 2026-03-17

### Bug Fixes

- 修复断点续传多个数据一致性问题
- 修复断点续传恢复后文件校验失败的问题
- 修复断点续传恢复时发送方进度从0开始的问题
- 修复暂停传输不通知对端的问题
- 修复详情页暂停时序 bug 及暂停会话的进度/恢复显示
- 修复 TransferStatus 类型重复定义及暂停时序 bug
- 修复接受配对请求失败时仍跳转设备页的问题
- 修复配对请求 pending channel 过期报错，优化配对流程
- 修复 macOS 上网络状态栏停止按钮样式丢失问题
### Features

- 实现发送方主动恢复断点续传
- 优化停止节点对话框响应式适配
- 优化节点对话框
- 优化停止节点对话框界面
- 优化历史记录项UI布局和交互体验
- 优化设备信息区域展示
### Refactor

- **transfer:** 提取断点续传辅助函数并优化前端操作逻辑
- 提取公共组件和宏，精简后端命令与前端页面代码
- 优化文件树组件 UI 细节与代码结构
- 优化文件树组件 UI 细节与代码结构
- 重构传输页面，提取共享组件并统一样式
## [0.4.3](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.4.3) - 2026-03-11

### Bug Fixes

- Android 打开保存目录改用 showViewDirDialog + 回退打开文件
## [0.4.2](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.4.2) - 2026-03-10

### Bug Fixes

- 修复移动端设置页面崩溃
### Features

- 引入 SaveLocation 枚举，修复移动端打开文件夹报错
## [0.4.1](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.4.1) - 2026-03-10

### Bug Fixes

- **android:** 修复移动端 safe-area 和 HTTP 访问问题
- 统一替换 app logo 为 app-icon.svg + 修复发送跳转和地址溢出
- 取消配对同步更新后端运行时状态
- Android 传输弹窗重复显示处理中 + 打开文件夹失败回退 + bump v0.3.0
- Android 文件操作调用链修复 + bump version
- open android folder
- android fs
- android fs
- android fs
- android check permission
- 修复路径拼接问题，使用 join 函数
- 添加 opener 权限到 mobile capability
- 添加缺失的 android.rs 模块文件
- 移除不存在的 android-fs:scope 权限
- 修复更新检查逻辑，避免重复弹窗
- 修复移动端 dialog 溢出问题
- 修复 Android APK 更新下载失败及通知栏无进度问题
- 修复 js-md5 导入方式，使用命名导出兼容 CI 环境
- 修复移动端自动升级 + 密钥环境变量化 + 精简多语言
- 在 UpgradeLink 同步前发布 Release 解决草稿资产不可访问问题
- 适配 AppUpdate 4.3.6 Builder API
- 升级 Kotlin 编译器到 2.1.0 修复 Android 构建
- 修复 Android 更新插件注册方式
- TypeScript 类型错误
- use latestVersion in mobile update button
- 限制 Android 构建目标为 aarch64
- 移除 Android 构建的 --apk 参数
- 配对请求超时设为 180 秒，修复 channel closed 错误
- 配对请求超时设为 180 秒，修复 channel closed 错误
### Documentation

- 添加断点续传代码审查报告
- 重写 README，优化项目介绍和结构
- 更新 README 反映 v0.3.0 进度
- 添加端到端加密实现博客
- add skill
- 同步配对模块文档与实际代码实现
- 更新 CLAUDE.md 并添加 MCP 与配对流程文档
- add skill
- 同步配对模块文档与实际代码实现
- 更新 CLAUDE.md 并添加 MCP 与配对流程文档
### Features

- **db:** SeaORM 2.0 实体定义 + bitmap 断点续传 + 场景设计文档
- **tauri:** 更新依赖包版本并添加mac_address支持
- 引导节点设置 + 网络状态展示增强
- PeerId 显示修复 + 配对路由守卫 + 自动启动节点设置
- 启动时自动恢复已配对设备的跨网络连接
- 替换应用图标为钴蓝电光配色方案
- prepare_send 字节级进度反馈 + UI 进度条
- 添加 file_source 模块 + 传输系统重构设计文档
- integration db
- add mobile capabilities configuration
- 传输完成后支持在文件夹中显示文件
- 优化移动端接收弹窗样式，添加 forceDialog 支持
- 添加文件传输设置区域
- Toast 改为顶部显示并添加关闭按钮
- 添加传输接收弹窗和详情页面
- 前端感知 relay 状态，更新博客文档
- 修复跨网络 dial 失败，添加 relay circuit reservation
- 实现分块传输核心（sender/receiver/progress）
- 文件选择后端实现 + 文件树 UI 优化
- 文件传输页面重构 + 文件树组件 + 代码优化
- 添加文件传输前端页面 + 移动端适配
- 添加文件传输加密模块 + 设计文档
- Android 更新插件重构 + 包名迁移至 com.yexiyue.swarmdrop
- 集成 UpgradeLink 自动更新系统
- 添加移动端安全区域适配指南及相关实现
- 实现全平台自动更新 + Android 签名 + CI/CD 发布工作流
- 集成系统通知插件，配对请求后台推送 + 修复 toast 反馈
- 前端配对命令同步后端 addrs 参数
- 配对 UI 重构、设备信息增强与取消配对功能
- 侧边栏折叠功能与 shadcn UI 组件升级
- 实现全平台自动更新 + Android 签名 + CI/CD 发布工作流
- 集成系统通知插件，配对请求后台推送 + 修复 toast 反馈
- 前端配对命令同步后端 addrs 参数
- 配对 UI 重构、设备信息增强与取消配对功能
- 侧边栏折叠功能与 shadcn UI 组件升级
- 完善 UI 设计稿和网络管理器重构
- 移动端适配、多语言支持与设置页面
### Miscellaneous

- update submodule
- 补充 en/zh-TW 翻译 + 更新 todo 版本规划
- 新增事件名常量模块和传输系列技术文章
- README 图标改用 SVG 格式
- bump version
- update Cargo.lock for v0.1.2
- 更新 swarm-p2p-core 子模块 (NodeEvent Deserialize 支持)
- 替换 Claude skills，添加 Rust/Tauri 技能包
- 更新 swarm-p2p-core 子模块 (on_event 值匹配优化)
- 更新 swarm-p2p-core 子模块 (Kad 地址同步 + 新命令)
- 更新 swarm-p2p-core 子模块 (NodeEvent Deserialize 支持)
- 替换 Claude skills，添加 Rust/Tauri 技能包
- 更新 swarm-p2p-core 子模块 (on_event 值匹配优化)
- 更新 swarm-p2p-core 子模块 (Kad 地址同步 + 新命令)
### Refactor

- 前端代码质量改进 + 补全 i18n 翻译
- Rust 代码质量改进
- 配对/传输拒绝原因改为类型化枚举
- 配对码改为单例模式，修复配对逻辑缺陷，优化传输进度追踪
- Android 文件打开链路重构 + 传输完成事件增强 + 代码简化
- 新增 file_sink 模块 + 接收端 OOP 重构 + clippy 修复
- 传输系统两阶段扫描重构 + Android 文件选择器优化
- 统一 session_id/prepared_id 类型为 uuid::Uuid
- 移除未使用的 android 模块
- 优化前端代码性能和代码规范
- Transfer 模块重组 + 前端代码审查简化
- 统一使用 UpgradeLink 更新系统，移除旧 update-store
- 直接从 rawJson 读取 upgradeType
- 代码审查清理 — 精简错误类型、消除冗余、增强连接推断
- 后端驱动设备管理，Channel 轮询改为 Event 推送
- 简化主题切换，移除 Zustand 冗余管理
- 配对模块 DHT 命名空间 + 在线宣告重构
- 配对模块路由化重构 + 入站请求状态分离
- 代码审查清理 — 精简错误类型、消除冗余、增强连接推断
- 后端驱动设备管理，Channel 轮询改为 Event 推送
- 简化主题切换，移除 Zustand 冗余管理
- 配对模块 DHT 命名空间 + 在线宣告重构
- 配对模块路由化重构 + 入站请求状态分离
## [0.4.0](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.4.0) - 2026-03-09

### Features

- 添加 MCP Server 支持，实现 AI 助手控制文件传输
## [0.3.8](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.8) - 2026-03-07

### Features

- 前端 NetworkStatus 类型新增 relayPeers 字段
- NetworkStatus 新增 relay_peers 字段
- DeviceManager 通过 agent_version 过滤非 SwarmDrop 节点
- 添加 OsInfo::is_swarmdrop_agent() 辅助方法
- 更新弹窗和设置页支持 markdown 渲染更新日志
### Refactor

- 简化 bootstrap_connected 状态管理，改为动态计算
- 用 identify agent_version 替代硬编码 PeerId 识别引导节点，升级 relay_peers 追踪
- 传输历史过滤下拉框替换为 shadcn/ui Select
- 统一移动端和桌面端更新 UI
## [0.3.7](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.7) - 2026-03-05

### Features

- 集成 git-cliff 自动生成更新日志
- 传输暂停按钮 + 侧边栏主题语言切换 + 网络状态栏可点击
## [0.3.6](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.6) - 2026-03-04

### Features

- 完善断点续传功能，补充国际化翻译
## [0.3.5](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.5) - 2026-03-04

### Bug Fixes

- 修复传输历史为空及 DB 错误静默处理，bump v0.3.5
## [0.3.4](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.4) - 2026-03-04

### Features

- 断点续传完整实现 + 传输流程优化
## [0.3.3](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.3) - 2026-03-03

### Documentation

- 添加断点续传代码审查报告
### Features

- **db:** SeaORM 2.0 实体定义 + bitmap 断点续传 + 场景设计文档
### Miscellaneous

- update submodule
## [0.3.2](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.2) - 2026-02-27

### Bug Fixes

- 统一替换 app logo 为 app-icon.svg + 修复发送跳转和地址溢出
### Documentation

- 重写 README，优化项目介绍和结构
## [0.3.1](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.1) - 2026-02-27

### Bug Fixes

- 取消配对同步更新后端运行时状态
### Documentation

- 更新 README 反映 v0.3.0 进度
### Features

- 引导节点设置 + 网络状态展示增强
- PeerId 显示修复 + 配对路由守卫 + 自动启动节点设置
- 启动时自动恢复已配对设备的跨网络连接
- 替换应用图标为钴蓝电光配色方案
### Miscellaneous

- 补充 en/zh-TW 翻译 + 更新 todo 版本规划
- 新增事件名常量模块和传输系列技术文章
- README 图标改用 SVG 格式
### Refactor

- 前端代码质量改进 + 补全 i18n 翻译
- Rust 代码质量改进
- 配对/传输拒绝原因改为类型化枚举
- 配对码改为单例模式，修复配对逻辑缺陷，优化传输进度追踪
## [0.3.0](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.3.0) - 2026-02-26

### Bug Fixes

- Android 传输弹窗重复显示处理中 + 打开文件夹失败回退 + bump v0.3.0
## [0.2.25](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.25) - 2026-02-26

### Bug Fixes

- Android 文件操作调用链修复 + bump version
## [0.2.24](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.24) - 2026-02-26

### Refactor

- Android 文件打开链路重构 + 传输完成事件增强 + 代码简化
- 新增 file_sink 模块 + 接收端 OOP 重构 + clippy 修复
## [0.2.23](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.23) - 2026-02-26

### Documentation

- 添加端到端加密实现博客
### Features

- prepare_send 字节级进度反馈 + UI 进度条
- 添加 file_source 模块 + 传输系统重构设计文档
- integration db
### Refactor

- 传输系统两阶段扫描重构 + Android 文件选择器优化
- 统一 session_id/prepared_id 类型为 uuid::Uuid
## [0.2.18](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.18) - 2026-02-20

### Bug Fixes

- open android folder
## [0.2.17](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.17) - 2026-02-20

### Bug Fixes

- android fs
### Miscellaneous

- bump version
## [0.2.16](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.16) - 2026-02-20

### Bug Fixes

- android fs
- android fs
## [0.2.15](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.15) - 2026-02-20

### Bug Fixes

- android check permission
- 修复路径拼接问题，使用 join 函数
- 添加 opener 权限到 mobile capability
## [0.2.14](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.14) - 2026-02-19

### Bug Fixes

- 添加缺失的 android.rs 模块文件
- 移除不存在的 android-fs:scope 权限
- 修复更新检查逻辑，避免重复弹窗
- 修复移动端 dialog 溢出问题
### Refactor

- 移除未使用的 android 模块
## [0.2.13](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.13) - 2026-02-19

### Features

- add mobile capabilities configuration
## [0.2.12](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.12) - 2026-02-19

### Features

- 传输完成后支持在文件夹中显示文件
- 优化移动端接收弹窗样式，添加 forceDialog 支持
## [0.2.11](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.11) - 2026-02-19

### Features

- 添加文件传输设置区域
## [0.2.10](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.10) - 2026-02-19

### Features

- Toast 改为顶部显示并添加关闭按钮
- 添加传输接收弹窗和详情页面
### Refactor

- 优化前端代码性能和代码规范
## [0.2.9](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.9) - 2026-02-19

### Features

- 前端感知 relay 状态，更新博客文档
## [0.2.8](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.8) - 2026-02-19

### Features

- 修复跨网络 dial 失败，添加 relay circuit reservation
## [0.2.7](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.7) - 2026-02-19

### Bug Fixes

- 修复 Android APK 更新下载失败及通知栏无进度问题
## [0.2.6](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.6) - 2026-02-18

### Features

- 实现分块传输核心（sender/receiver/progress）
## [0.2.5](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.5) - 2026-02-18

### Refactor

- Transfer 模块重组 + 前端代码审查简化
## [0.2.4](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.4) - 2026-02-18

### Bug Fixes

- 修复 js-md5 导入方式，使用命名导出兼容 CI 环境
### Features

- 文件选择后端实现 + 文件树 UI 优化
### Refactor

- 统一使用 UpgradeLink 更新系统，移除旧 update-store
## [0.2.3](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.3) - 2026-02-18

### Features

- 文件传输页面重构 + 文件树组件 + 代码优化
## [0.2.2](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.2) - 2026-02-18

### Bug Fixes

- 修复移动端自动升级 + 密钥环境变量化 + 精简多语言
## [0.2.1](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.1) - 2026-02-18

### Features

- 添加文件传输前端页面 + 移动端适配
- 添加文件传输加密模块 + 设计文档
## [0.2.0](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.2.0) - 2026-02-16

### Bug Fixes

- 在 UpgradeLink 同步前发布 Release 解决草稿资产不可访问问题
- 适配 AppUpdate 4.3.6 Builder API
- 升级 Kotlin 编译器到 2.1.0 修复 Android 构建
- 修复 Android 更新插件注册方式
- TypeScript 类型错误
### Features

- Android 更新插件重构 + 包名迁移至 com.yexiyue.swarmdrop
- 集成 UpgradeLink 自动更新系统
### Refactor

- 直接从 rawJson 读取 upgradeType
## [0.1.2](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.1.2) - 2026-02-14

### Bug Fixes

- **android:** 修复移动端 safe-area 和 HTTP 访问问题
- use latestVersion in mobile update button
### Miscellaneous

- update Cargo.lock for v0.1.2
## [0.1.1](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.1.1) - 2026-02-14

### Features

- 添加移动端安全区域适配指南及相关实现
## [0.1.0](https://github.com/swarm-apps/SwarmDrop/releases/tag/v0.1.0) - 2026-02-14

### Bug Fixes

- 限制 Android 构建目标为 aarch64
- 移除 Android 构建的 --apk 参数
- 配对请求超时设为 180 秒，修复 channel closed 错误
- 配对请求超时设为 180 秒，修复 channel closed 错误
- Android 节点启动失败
### Documentation

- add skill
- 同步配对模块文档与实际代码实现
- 更新 CLAUDE.md 并添加 MCP 与配对流程文档
- add skill
- 同步配对模块文档与实际代码实现
- 更新 CLAUDE.md 并添加 MCP 与配对流程文档
- 添加设备配对系统实现博客
### Features

- 实现全平台自动更新 + Android 签名 + CI/CD 发布工作流
- 集成系统通知插件，配对请求后台推送 + 修复 toast 反馈
- 前端配对命令同步后端 addrs 参数
- 配对 UI 重构、设备信息增强与取消配对功能
- 侧边栏折叠功能与 shadcn UI 组件升级
- 实现全平台自动更新 + Android 签名 + CI/CD 发布工作流
- 集成系统通知插件，配对请求后台推送 + 修复 toast 反馈
- 前端配对命令同步后端 addrs 参数
- 配对 UI 重构、设备信息增强与取消配对功能
- 侧边栏折叠功能与 shadcn UI 组件升级
- 完善 UI 设计稿和网络管理器重构
- 移动端适配、多语言支持与设置页面
- 实现设备配对系统 (Phase 2)
- 添加用户偏好设置 Store (tauri-plugin-store 持久化)
- Android 构建支持与 libsodium 预编译库
- mdns devices
- implement authentication system with biometric unlock
- add Claude Code skills and auth design docs
- add network dialog and improve sidebar with shadcn/ui
- devices
- init android
- integrate shadcn/ui, Tailwind CSS v4, and TanStack Router
### Miscellaneous

- 更新 swarm-p2p-core 子模块 (NodeEvent Deserialize 支持)
- 替换 Claude skills，添加 Rust/Tauri 技能包
- 更新 swarm-p2p-core 子模块 (on_event 值匹配优化)
- 更新 swarm-p2p-core 子模块 (Kad 地址同步 + 新命令)
- 更新 swarm-p2p-core 子模块 (NodeEvent Deserialize 支持)
- 替换 Claude skills，添加 Rust/Tauri 技能包
- 更新 swarm-p2p-core 子模块 (on_event 值匹配优化)
- 更新 swarm-p2p-core 子模块 (Kad 地址同步 + 新命令)
- update subproject commit reference in libs
### Refactor

- 代码审查清理 — 精简错误类型、消除冗余、增强连接推断
- 后端驱动设备管理，Channel 轮询改为 Event 推送
- 简化主题切换，移除 Zustand 冗余管理
- 配对模块 DHT 命名空间 + 在线宣告重构
- 配对模块路由化重构 + 入站请求状态分离
- 代码审查清理 — 精简错误类型、消除冗余、增强连接推断
- 后端驱动设备管理，Channel 轮询改为 Event 推送
- 简化主题切换，移除 Zustand 冗余管理
- 配对模块 DHT 命名空间 + 在线宣告重构
- 配对模块路由化重构 + 入站请求状态分离
- ui
- use biometry plugin for secure password storage
### Merge

- 合并上游 develop 分支并解决 mod.rs 冲突

