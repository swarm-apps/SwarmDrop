# error-reporting-contract Specification

## Purpose
TBD - created by archiving change failure-semantics-contract. Update Purpose after archive.
## Requirements
### Requirement: 错误 kind 是判别码，用户文案由各端本地化生成

后端错误 SHALL 以 `{ kind, message }` 的形状跨越每一条宿主边界（Tauri IPC / uniffi / wasm-bindgen）。

`kind` SHALL 是语言无关的稳定判别码；`message` SHALL 只作开发者与日志用途。

三端 UI SHALL NOT 把 `message` 直接渲染给用户 —— 它是 Rust 侧写的、随开发语言走的技术描述，
在任何非该语言的界面下都会露馅。每一端 SHALL 各自持有一份 `kind` → 本地化文案的映射表，
未命中的 kind SHALL 落到该端的通用兜底文案。

#### Scenario: 英文界面下发生后端错误

- **WHEN** 用户把界面语言设为 English，触发一个返回 `{ kind: "SessionNotFound", message: "收件箱条目不存在" }` 的操作
- **THEN** 用户看到的是该端 English catalog 里 `SessionNotFound` 对应的那句话；界面上 SHALL NOT
  出现 `message` 里的任何字符

#### Scenario: 后端新增一个 kind，某端还没补文案

- **WHEN** 后端新增 kind 而某一端的映射表尚未补上对应条目
- **THEN** 该端展示通用兜底文案，SHALL NOT 展示原始 `message`，也 SHALL NOT 展示 kind 本身

### Requirement: 每个 kind 必须对应一条用户能照做的建议

新增错误 kind 的判据 SHALL 是「UI 能据此给出与其他 kind 不同的、用户真能照做的建议」。

不满足该判据的失败 SHALL 归入所在域的「其余」变体（传输域为 `Transfer`），而不是各自造一个 kind。

承载「其余」的变体 SHALL 在其文档注释里写明判据，使后续新增失败模式时有可依据的归类标准。

#### Scenario: 内部失败（锁中毒 / 句柄类型异常 / 序列化失败）

- **WHEN** 发生用户无从处置的内部失败
- **THEN** 它归入「其余」变体，UI 展示通用兜底文案；SHALL NOT 为其单独造 kind

#### Scenario: 磁盘写入失败

- **WHEN** 落盘因空间不足或权限问题失败
- **THEN** 它归入 `StorageFailed`，UI 能给出「检查磁盘空间或更换保存位置」这类与其他 kind 不同的建议

### Requirement: 结构化的业务结果不得压成错误

后端已经用结构化类型表达的业务结果 SHALL 以结构化形式抵达 UI，包括
`PairingResponse::Refused { reason }` 与 `OfferResult { accepted: false, reason }`。

宿主层 SHALL NOT 把它们压成某个错误 kind 加一句写死的自然语言 —— 那会同时丢掉判别信息
（reason 是什么）与本地化能力（那句话固定在一种语言上），并且往往落到语义相反的 kind 上
（对方点了拒绝，被显示成「网络错误」）。

#### Scenario: 对方拒绝配对

- **WHEN** 邀请方在确认卡上点了拒绝
- **THEN** 受邀方 UI 展示按 `PairingRefuseReason` 本地化的文案；SHALL NOT 展示「网络错误」
  或任何未经本地化的固定字符串

#### Scenario: 三端一致

- **WHEN** 同一次拒绝分别发生在桌面、移动、Web
- **THEN** 三端都按 reason 出文案，SHALL NOT 出现「一端说清楚了原因、另一端说网络错误」这类分叉

