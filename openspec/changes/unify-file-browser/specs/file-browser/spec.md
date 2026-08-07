## ADDED Requirements

### Requirement: 文件浏览器的 React DOM 表现层 SHALL 只有一份实现

桌面（`src/`）与 Web（`docs/app/app`）MUST 共用 `packages/file-browser` 导出的同一套组件与文案。任一端 MUST NOT 自建平行的树形 / 网格 / 文件卡片实现。

移动端（`mobile/src`）保留独立的 React Native 表现层——两个渲染栈不共用 JSX——但 MUST 消费同一份纯逻辑（见下一条）。

#### Scenario: 桌面与 Web 渲染同一份组件

- **WHEN** 桌面传输详情与 Web 传输详情各自展示同一条会话的文件明细
- **THEN** 两端 MUST 渲染同一个 `FileBrowser` 组件，视图切换、计数行、空态文案的行为一致

#### Scenario: 仓库中不存在第二份 React DOM 实现

- **WHEN** 本变更完成后检索文件浏览器组件
- **THEN** `src/components/file-browser/` 与 `docs/app/app/_components/` 下的 `TransferFileList` MUST 已被删除，仓库中 React DOM 的文件浏览器实现只有 `packages/file-browser/`

### Requirement: 建树、媒体判定与来源归一 SHALL 三端共用一份纯逻辑

`packages/shared-view/src/file-browser/` MUST 提供路径归一与建树（`tree-data`）、图片 / 视频扩展名判定（`media-type`）、数据来源归一（`adapters`）、缩略图契约（`thumbnail`）四组纯函数，三端 MUST 全部消费它。

这些模块 MUST 满足 `shared-view` 既有的两道门禁：不引用任何 DOM / React Native / Node API，不出现任何非相对路径 import。

#### Scenario: 移动端消费共享逻辑

- **WHEN** 移动端的 `tree-data.ts` / `media-type.ts` / `adapters.ts` 被加载
- **THEN** 它们 MUST 只做从 `@swarmdrop/shared-view` 的重导出与移动端特有的类型转换，不含第二份算法实现

#### Scenario: 门禁拦住平台依赖

- **WHEN** 有人往 `shared-view/src/file-browser/` 加入 `document` / `createImageBitmap` / 非相对路径 import
- **THEN** `pnpm check:shared-view` MUST 失败

### Requirement: 系统 SHALL 使用单一的 FileBrowserItem 模型

所有来源的文件数据 MUST 在进入表现层之前归一为同一个 `FileBrowserItem` 形状。表现层 MUST NOT 通过字段探测（如 `"transferred" in file`）判断数据来自哪个来源。

`status` 取三端并集：`idle`、`waiting`、`transferring`、`paused`、`completed`、`cancelled`、`error`、`missing`。`size` 类型为 `number`；移动端的 `bigint` MUST 在移动端 adapter 边界转换。

#### Scenario: 表现层不做形状嗅探

- **WHEN** 表现层渲染一行文件的已传字节数与状态
- **THEN** 它 MUST 直接读 `FileBrowserItem` 的固定字段，源码中 MUST NOT 出现 `in` 运算符形式的形状判别

#### Scenario: 移动端的 bigint 在边界转换

- **WHEN** 移动端把 uniffi 返回的文件记录转成 `FileBrowserItem`
- **THEN** 转换 MUST 在移动端 adapter 内完成，`FileBrowserItem.size` 交出时已是 `number`

### Requirement: 传输进度 SHALL 作为投影之上的覆盖层

由传输会话构造文件清单时，`projection.files` MUST 作为骨架（文件身份、名称、大小、相对路径的唯一来源），`TransferProgressEvent` MUST 只覆盖在途的已传字节数与逐文件状态。二者 MUST NOT 以「二选一」的方式使用。

会话处于终态时 MUST 忽略进度事件，该判定 MUST 收在归一函数内部，MUST NOT 由各消费点分别携带。

#### Scenario: 进行中的会话显示实时逐文件进度

- **WHEN** 一条 `active` 会话正在传输，且已收到进度事件
- **THEN** 文件清单的条目数与身份 MUST 来自 `projection.files`，每条的已传字节数与状态 MUST 来自进度事件中 `fileId` 匹配的那一项

#### Scenario: 终态会话忽略残留的进度快照

- **WHEN** 一条会话已进入终态，而 store 中仍留有该会话此前的进度事件
- **THEN** 归一函数 MUST 忽略该进度事件，文件清单完全由 `projection.files` 决定

#### Scenario: 切换会话不串数据

- **WHEN** 用户在会话列表中从会话 A 切到会话 B，再切回 A
- **THEN** 详情侧展示的文件清单 MUST 恒等于该会话 `projection.files` 的内容，条目数 MUST NOT 随切换次数增长，MUST NOT 出现属于另一条会话的文件

### Requirement: 文件浏览器 SHALL 提供树形与网格两种视图

组件 MUST 支持 `tree` 与 `grid` 两种视图并提供切换控件。视图选择 MUST 按 `FileBrowserScope`（`send` / `transfer` / `inbox`）分别持久化，三端 MUST 使用同一套 scope 枚举。

#### Scenario: Web 端具备双视图

- **WHEN** 用户在 Web 端打开传输详情、收件箱或发送页的文件区
- **THEN** MUST 能在树形与网格之间切换，切换结果 MUST 在刷新后仍然生效

#### Scenario: 不同 scope 的视图偏好互不影响

- **WHEN** 用户把收件箱切成网格、传输详情保持树形
- **THEN** 两处 MUST 各自记住自己的选择

### Requirement: 共享组件的文案 SHALL 落入各端自己的 catalog

`packages/file-browser` 的组件 MUST 内联 Lingui 宏（`<Trans>` 与 `useLingui()` 的 `t`），MUST NOT 通过 props 接收 UI 文案，MUST NOT 依赖任一端的全局 i18n 单例。

桌面与 Web 的 `lingui.config.ts` MUST 把该包源码纳入 `include`，使其文案被提取进各端既有的 catalog。三端 catalog 保持独立这条既有约定 MUST NOT 因此改变。

#### Scenario: 两端各自提取到共享组件的文案

- **WHEN** 在桌面与 Web 分别执行 `pnpm i18n:extract`
- **THEN** 两端的 `.po` 目录中 MUST 各自出现共享组件贡献的 msgid

#### Scenario: 三端 Lingui 运行时同版本

- **WHEN** 检查三端的 `@lingui/core` 与 `@lingui/react` 版本
- **THEN** 三端 MUST 同为 6.x
