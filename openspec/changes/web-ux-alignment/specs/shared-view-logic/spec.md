## ADDED Requirements

### Requirement: 共享视图逻辑包零平台依赖

`packages/shared-view` SHALL 不依赖 React、React Native、DOM API、Node API、Tauri API、
wasm-bindgen 产物，也 SHALL NOT import 任何一端的 bindings 模块（`src/lib/bindings.ts`、
`swarmdrop-web`、`react-native-swarmdrop-core`）。

入参型别 SHALL 用结构化类型声明（只声明用到的字段），使三端各自的 `Device` 型别都能结构化赋值，
而无需共享包知道它们从哪来。

#### Scenario: 包内出现平台依赖

- **WHEN** `packages/shared-view` 的任一源文件 import 了 React / RN / DOM / Node / 任一端 bindings
- **THEN** 包的构建或 lint SHALL 失败，且失败信息指出被禁止的 import

#### Scenario: 三端各自的 Device 型别都能传入

- **WHEN** 分别把桌面 `bindings.ts` 的 `Device`、Web `swarmdrop-web` 的 `Device`、移动
  `react-native-swarmdrop-core` 的 `MobileDevice` 传给同一个共享函数
- **THEN** 三者 SHALL 全部通过类型检查，无需任何 `as` 断言或适配层

### Requirement: 纯视图逻辑收口于共享包，各端不得再长本地副本

以下逻辑 SHALL 只存在于 `packages/shared-view` 一处：设备显示名归一、别名与分组解析及排序、
同名消歧的次级身份提示、信任级别归一与「是否可发送」判定、字节大小 / 传输速率 / 延迟 / 时长的格式化。

各端的既有同名模块 SHALL 改为对共享包的 re-export，或直接由调用点改 import；SHALL NOT 保留
本地实现。

#### Scenario: 三端调用同一函数得到同一结果

- **WHEN** 桌面、移动、Web 三端对同一份设备数据调用 `deviceDisplayName`
- **THEN** 三端 SHALL 返回完全相同的字符串

#### Scenario: 新增纯视图逻辑时的归属判定

- **WHEN** 需要新增一个「只依赖 `Device` 等 DTO、不碰平台 API」的展示派生函数
- **THEN** 它 SHALL 放进 `packages/shared-view`，而 SHALL NOT 放进任一端的 `lib/`

#### Scenario: 平台相关逻辑不进共享包

- **WHEN** 某个函数需要读取平台偏好、调用 IPC、访问文件系统或依赖 i18n 运行时
- **THEN** 它 SHALL 留在对应端，共享包 SHALL NOT 接收它

### Requirement: 共享包被三个 workspace 同时消费

`packages/shared-view` SHALL 位于仓库根 pnpm workspace，并 SHALL 同时可被根 workspace（桌面 `src/`）、
`docs/` workspace（Web）与 `mobile/` workspace（React Native）解析。

`docs/` 与 `mobile/` 作为独立 workspace，SHALL 经显式的本地链接引用它，而不依赖「向上查找被劫持」
这类隐式行为。

#### Scenario: docs 构建能解析共享包

- **WHEN** 在 `docs/` 下执行 `pnpm install` 后运行 `pnpm build`
- **THEN** 构建 SHALL 成功，且共享包的源码 SHALL 被正确转译（不因是 workspace 外的 TS 源而报错）

#### Scenario: 移动端类型检查能解析共享包

- **WHEN** 在 `mobile/` 下执行 `pnpm typecheck`
- **THEN** 类型检查 SHALL 成功且共享包的型别可见

#### Scenario: 根 workspace 的 Rust 检查不受影响

- **WHEN** 新增 `packages/shared-view` 后运行 `cargo check --workspace --all-targets`
- **THEN** 结果 SHALL 与新增前一致（新增的是 JS 包，不进 Cargo workspace）

### Requirement: 共享包的单测是三端的唯一事实源

三端现有测试合并去重后的用例 SHALL 落在共享包内并随包运行，各端 SHALL NOT 保留针对同一逻辑的
重复测试。来源包括 `src/lib/device-name.test.ts`、`src/lib/device-organization.test.ts` 及移动端等价物。

#### Scenario: 共享逻辑回归被单点拦截

- **WHEN** 修改共享包中的设备显示名归一逻辑并引入回归
- **THEN** 共享包的单测 SHALL 失败，且该失败对三端都成立（无需在三处各跑一次才发现）
