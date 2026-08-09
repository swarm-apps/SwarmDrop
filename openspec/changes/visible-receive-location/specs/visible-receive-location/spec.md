## ADDED Requirements

### Requirement: 接收落点必须是用户可见位置

所有平台的接收落点 MUST 位于用户可通过系统文件管理器（或平台等价出口）访问的位置。任何实现 MUST NOT 把应用私有目录作为接收落点，也 MUST NOT 在配置缺失时静默回退到私有目录。

#### Scenario: iOS 落点在「文件」App 中可见

- **WHEN** 用户在 iOS 上完成一次接收
- **THEN** 收到的文件出现在「文件」App 的 `On My iPhone / SwarmDrop` 下
- **AND** 用户可以在系统文件管理器中直接打开、移动或删除它

#### Scenario: Android 落点在系统文件管理器中可见

- **WHEN** 用户在 Android 上完成一次接收
- **THEN** 收到的文件位于用户此前选定的 SAF 目录下
- **AND** 系统文件管理器可浏览到该目录

#### Scenario: 不存在私有目录回退

- **WHEN** 接收落点尚未配置
- **THEN** 系统 MUST NOT 使用应用私有目录完成接收
- **AND** 系统 MUST 引导用户配置落点

### Requirement: 应用私有数据不得存放于用户可见接收区

数据库、暂存文件及任何应用内部状态 MUST 存放在应用私有数据区，与用户可见接收区物理分离。用户在系统文件管理器中 MUST NOT 看到这些内部数据。

#### Scenario: iOS 数据库与暂存不出现在「文件」App

- **WHEN** 用户在「文件」App 中浏览 `On My iPhone / SwarmDrop`
- **THEN** 该目录下只有收到的文件
- **AND** `swarmdrop.db`、`swarmdrop.db-wal`、`swarmdrop.db-shm` 与 `staging/` 均不可见

#### Scenario: 暂存区仍跨会话存活

- **WHEN** 一次接收在中途中断，用户数天后回到应用点击恢复
- **THEN** 暂存的已收数据仍然存在
- **AND** 续传从中断处继续，而非重新开始

### Requirement: 接收落点状态是三态且必须穷尽处理

接收落点的查询结果 MUST 是一个显式的三态值：`ready`（含可用 URI）、`unconfigured`（从未配置）、`revoked`（曾配置但授权已失效，含原 URI）。调用方 MUST 穷尽处理全部三个分支；新增状态变体时 MUST 在编译期暴露未处理的调用点。

#### Scenario: 未配置与已失效给出不同引导

- **WHEN** 落点状态为 `unconfigured`
- **THEN** 界面提示用户「选择一个接收目录」
- **WHEN** 落点状态为 `revoked`
- **THEN** 界面说明原目录已不可用，并显示原路径帮助用户定位

#### Scenario: 新增状态变体触发编译失败

- **WHEN** 开发者向落点状态类型新增一个变体
- **THEN** 所有未处理该变体的分派点在编译期报错

### Requirement: Android 接收目录由用户选定且授权跨重启存活

Android 的接收目录 MUST 由用户经系统目录选择器指定，且实现 MUST 取得可持久化的 URI 授权，使其在应用重启后仍然有效。

#### Scenario: 重启后授权仍然有效

- **WHEN** 用户选定接收目录后重启应用
- **THEN** 落点状态仍为 `ready`
- **AND** 无需再次选择目录即可接收文件

#### Scenario: 用户取消目录选择

- **WHEN** 用户在系统目录选择器中取消
- **THEN** 落点状态保持 `unconfigured`
- **AND** 界面停留在配置步骤并说明为何需要这次选择

### Requirement: SAF 授权失效必须在接收前被发现

接受入站传输请求前，实现 MUST 校验接收落点当前可写。授权失效 MUST 在接受动作之前暴露给用户，MUST NOT 表现为接受之后的静默失败。

#### Scenario: 授权失效时拦截接受动作

- **WHEN** 用户清除应用数据或删除了目标目录后收到一个传输请求
- **AND** 用户点击接受
- **THEN** 系统检测到落点不可用，进入 `revoked` 态
- **AND** 先引导用户重选目录，重选成功后才继续接受流程

#### Scenario: 重选目录后传输正常完成

- **WHEN** 用户在 `revoked` 引导中重新选定了一个目录
- **THEN** 落点状态变为 `ready`
- **AND** 此前被拦截的传输请求可以正常接受并完成

### Requirement: 引导完成状态由前置条件派生

引导流程 MUST 由一组有序步骤构成，每步带一个可判定的满足条件。是否完成引导 MUST 从这些条件派生，MUST NOT 依赖一个独立持久化的完成标记。路由 MUST 指向第一个未满足的步骤。

#### Scenario: 存量用户自动补跑新增步骤

- **WHEN** 一个此前已完成引导的 Android 用户升级到包含接收目录步骤的版本
- **THEN** 应用把该用户领到接收目录选择步骤
- **AND** 已满足的步骤（设备名等）不再重复询问

#### Scenario: iOS 跳过不适用的步骤

- **WHEN** iOS 用户首次启动应用
- **THEN** 引导中不出现接收目录选择步骤
- **AND** 步骤指示器反映的是本平台的实际步骤数

#### Scenario: 清空配置后回到对应步骤

- **WHEN** Android 用户在设置中清空了接收目录
- **THEN** 落点状态回到 `unconfigured`
- **AND** 下一次需要接收时用户被领回目录选择流程

### Requirement: 落点形态按平台唯一确定

治本后每个平台的 `save_dir` MUST 只有一种 URI 形态：iOS 恒为 `file://`，Android 恒为 `content://`。publish 路径的分支 MUST 依据平台判据，MUST NOT 依据运行时对 URI 前缀的推测来决定走哪条实现。

#### Scenario: iOS 走本地发布路径

- **WHEN** iOS 上一个文件收齐并准备发布
- **THEN** 发布由 Rust 侧直接完成，不经过宿主 JS 层

#### Scenario: Android 走 SAF 发布路径

- **WHEN** Android 上一个文件收齐并准备发布
- **THEN** 发布经宿主的 SAF 实现完成

### Requirement: 「在文件夹中显示」入口的可用性判据

只要落点状态为 `ready`，「在文件夹中显示 / 打开文件夹」入口 MUST 可用。实现 MUST NOT 渲染一个注定失败的入口。

#### Scenario: 收件箱详情提供可用的打开文件夹入口

- **WHEN** 用户打开一条已完成接收的收件箱记录
- **AND** 落点状态为 `ready`
- **THEN** 「打开文件夹」入口可见且可点击
- **AND** 点击后唤起系统文件管理器定位到该目录

#### Scenario: 落点不可用时入口不渲染

- **WHEN** 落点状态为 `revoked`
- **THEN** 「打开文件夹」入口不渲染，或渲染为不可点击并说明原因

### Requirement: Web 端以 OPFS 为持有区、以下载为发布出口

Web 端 MUST 把 OPFS 作为已接收文件的持有区，并以浏览器下载作为交付给用户的发布出口。实现 MUST NOT 依赖 File System Access API 的目录选择能力，以免在不支持该 API 的浏览器上失去接收能力。

#### Scenario: 全浏览器可完成接收

- **WHEN** 用户在 Safari 或 Firefox 中接收文件
- **THEN** 接收正常完成并落入 OPFS
- **AND** 收件箱中可见该条目

#### Scenario: 批量导出

- **WHEN** 用户在收件箱中选择多个已接收文件并触发导出
- **THEN** 这些文件被交付到浏览器的下载出口
- **AND** 用户无需逐个点击下载
