# Rust 侧的中文串：它是不是 i18n 问题

> **状态：🟢 已决策并全部落地（2026-08-05）。** 实施见
> `openspec/changes/rust-string-boundary/`。结论是**方案 B（判别码化）**，
> 真正要动的只有 **3 条通道**——不是 1466 处。
>
> 实施中修正了本文两处事实错误，都以「勘误」标注在原处：`FatalError` 的构造点是
> **3 个**不是 1 个；`friendlyTransferError` 的正则不是「从不命中」，而是**会拿文件名
> 误命中**（更糟）。
>
> **调研中发现一个现存 bug**（不是 i18n 问题，但由同一个根因导致）：移动端把**文件名**
> 拼进了错误串再拿去做关键词匹配，于是一个叫 `Q3-cancel.xlsx` 的文件校验失败时，
> 用户看到的是「传输已取消」。详见通道 B。
>
> 本篇的前置事实来自刚落地的 `failure-semantics-contract`（commit `81c99617`），
> 它已经把最大的那条通道（命令返回值）关掉了。**同时修正该 change 的一处错误判断**：
> proposal 里写 `error_message` 通道「要动 wire」——核实为不动，它不进跨端协议。

## 先把数字打散

全仓 `.rs` 里含中文的**字符串字面量** 1466 行（另有 8913 行中文注释——那是本仓约定，
不在讨论范围）。按用途粗分之后，「是不是 i18n 问题」这个问题就自己回答了
（分类是启发式的，内联 `#[cfg(test)]` 里的测试数据有一部分落进了最后一行，
所以「测试」实际比表中更多、「运行时」更少——不影响结论）：

| 用途 | 行数 | 受众 | 是问题吗 |
|---|---|---|---|
| 测试数据 / 断言消息 | ~404 | 开发者 | 否 |
| `tracing` 日志 | ~142 | 开发者（日志文件） | 否 |
| `expect` / panic 消息 | ~113 | 开发者（崩溃栈） | 否 |
| `#[error(...)]` thiserror Display | 24 | **见下，分两类** | 一半是 |
| 运行时构造的错误 / 文案 | ~803 | **绝大多数仍是开发者** | 少数是 |

「运行时构造的错误」这 803 处才是唯一需要细看的。但它们里面的大头是
`AppError::Transfer(format!("..."))` 这类——而**上一轮改动之后，`AppError` 的 `message`
在三端都已经不进 UI 了**（只有 `kind` 进）。它现在的受众是 console 和日志。

所以真正的问题不是「有多少中文串」，是**哪些串会原样出现在用户眼前**。

## 通道清单（这才是答案）

追下来只有三条通道还在把 Rust 中文串**原样**送到 UI，加两条性质不同的：

| 通道 | 载体 | 到 UI 的路径 | 构造点 | 状态 |
|---|---|---|---|---|
| **A** | `AppError { kind, message }` | 只有 `kind` 到 UI，`message` 进 console | — | ✅ 已解决 |
| **B** | `ActorReport::FatalError(String)` → `session.error_message` | 三端 **6 个组件直接渲染** | **1 处** | ❌ |
| **C** | `expired_receive_reason()` → 同一列 | 同 B | **1 处** | ❌ |
| **D** | `inbox_title()` → `inbox_item.title` 列 | 收件箱列表直接渲染 | **1 处**（2 个分支） | ❌ |
| **E** | rust-i18n 托盘 / 通知 | 后端渲染 OS 表面 | 14 个键 | ✅ 但只在桌面 |
| **F** | MCP 工具描述与错误 | 给 **AI agent**，不是人 | ~97 处 | 另一类受众 |
| **G** | `crates/webrtc-p2p` 的 `#[error]` | 给**库使用者** | 19 处 | 独立问题 |

### B —— 致命失败原因，现有对策是死代码

> **勘误（实施时发现）**：本节原写「全仓只有**一个**构造点」，**是错的** ——
> 当时的 grep 只扫了 `crates/transfer/src/actor/`。实际是**三个**：下面这个、
> `flow/resume/mod.rs`（对端拒绝续传）、`flow/send.rs::mark_offer_fatal`（Offer 未送达）。
> 第二个尤其值得看：它经 `resume_reject_message()` 把一个**六变体的枚举**
> （`ResumeRejectReason`）摊平成六句中文再落库 —— 判别信息在 wire 上本来就是结构化的，
> 存储时降级成自由文本，到了 UI 又还原不回来。落地时判别码直接内嵌了那个 enum。

`ActorReport::FatalError` 的第一个构造点：

```rust
// crates/transfer/src/actor/receiver.rs:583
let msg = format!("文件最终化失败: {} (file_id={}): {}", file_info.name, file_info.file_id, e);
self.fail_session(epoch, msg.clone()).await;
```

它经 `coordinator.rs:279` 落进 `session.error_message`，然后被三端渲染：

- `src/components/transfer/session-panel.tsx:324`、`-session-row.tsx:237` —— **裸渲染**
- `docs/app/app/_components/send-panel.tsx:455`、`transfer-detail.tsx:446` —— **裸渲染**
- 移动端 3 处 —— 经 `LocalizedError`

**移动端那层「对策」比没有还糟——它会拿文件名当错误原因匹配。**
`friendlyTransferError`（`mobile/src/components/transfer/shared.tsx:265`）对整条
`errorMessage` 做 `toLowerCase()` 后跑 9 条关键词正则：

```ts
if (/reject/.test(m)) return <Trans>对方拒绝了这次传输</Trans>;
if (/(cancel|abort)/.test(m)) return <Trans>传输已取消</Trans>;
if (/(network|connection|connect|reset|broken pipe|unreachable|dial)/.test(m)) …
```

而它的输入是 `format!("文件最终化失败: {} (file_id={}): {}", file_info.name, …, e)`
——**文件名在里面**。于是：

| 用户的文件叫 | 命中 | 显示的原因 | 真实原因 |
|---|---|---|---|
| `Q3-cancel.xlsx` | `/(cancel\|abort)/` | 「传输已取消」 | 校验失败 |
| `network-diagram.png` | `/(network\|connect…)/` | 「网络连接中断,请确认两端在线后重试」 | 校验失败 |
| `read-me.txt` | `/(io error\|read\|write)/` | 「读写文件时出错」 | 可能是任何原因 |

这是**确定性的**，不是概率问题：文件名里出现那 30 个英文词中的任何一个，用户就会看到
一句与事实无关的解释。而「传输已取消」尤其有害——它把一次**数据损坏**说成用户自己的操作。

尾部 `{e}` 的匹配同样不可靠。`AppError` 的 `Display` 是英文的（`"IO error: {0}"`、
`"not found: {0}"`），所以有些分支确实会命中，但语义是错位的：`SessionNotFound`
命中 `/(not found|enoent…)/` → 显示「找不到**要传输的**文件,可能已被移动或删除」，
而这是**接收侧**的最终化失败，用户手上根本没有「要传输的文件」。

注释里写着的前提（「核心错误多为英文自由文本」）在某次改动后失效了，而正则匹配失效
**不会报错**——它只是安静地开始给出错误答案。这正是本仓刚吃过一次的亏
（`event-bus.ts` 的 `msg.includes("NodeNotStarted")`，上一轮改成了 `isErrorKind`）。

### C —— 会话过期回收

```rust
// crates/transfer/src/lib.rs:54
format!("会话超过 {} 天未恢复，已过期回收", retention_secs / 86_400)
```

同一列、同样的渲染路径。

### D —— 收件箱标题，麻烦在于它**已经落库**

```rust
// crates/transfer/src/inbox.rs:117
[] => "空传输".to_string(),
[file] => file.name.to_string(),          // ← 单文件无中文
[first, ..] => format!("{} 等 {} 个文件", first.name, files.len()),
```

两端各自在写入时调它（`storage-sql/src/inbox.rs:133`、`web/src/inbox.rs:166`），
结果存进 `inbox_item.title`，**并同时进 `inbox_search_index.title`**。

落库是有理由的（搜索索引要它），但代价是：**已写入的条目永远是中文**，切语言不会变。
这是三条通道里唯一有存量数据问题的。

### G —— 性质完全不同，不要混进来

`crates/webrtc-p2p` 的 19 处 `#[error("地址不可用：{0}")]` 之类，按 CLAUDE.md
是**「刻意不带 swarmdrop 前缀、不依赖任何 swarmdrop crate，将来要 subtree split 出去独立发布」**
的通用 libp2p transport。

一个准备发到 crates.io 的库，`Display` 是中文——这不是 i18n 问题，是**开源库的语言约定**问题。
它的读者是 Rust 生态的其他开发者，正确答案是**改成英文**，不是做 i18n。
与本篇其余部分**没有共用的解法**，应当单独处理。

## 三个方案

### 方案 A：全量 rust-i18n —— 否决

把中文串都进 `t!()`。1466 处里 1100+ 是日志、测试、panic 消息，翻译它们是纯负担；
而且日志本来就该用开发者语言，翻译日志会让搜索 issue 变得更难。

### 方案 B：判别码化 —— **推荐**

把三条通道的自由文本 `String` 换成**结构化判别码 + 参数**，Rust 只负责「是什么失败」，
文案由三端各自的 Lingui catalog 出。

这不是新体例，是**把上一轮已经验证过的做法接着往下用**：`AppError` 的 `kind` 就是这么做的，
三端的文案表（`src/lib/errors.ts`、`WEB_ERROR_KIND_LABEL`、`mobile/src/lib/errors.ts`）
已经建好了，扩展比新建便宜得多。

**成本比上一轮估的低。** `failure-semantics-contract` 的 proposal 里写这条通道
「要动 wire、DB 列、恢复逻辑与三端详情页」——**wire 那条是错的**，核实过：
`grep error_message crates/transfer/src/wire/ crates/transfer/src/protocol.rs` 零命中。
它是纯本地的 DB 列 + 本地事件，**跨端协议完全不动**。

形态（列类型不变，仍是 TEXT，存 JSON）：

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FailureCode {
    /// 通道 B：落盘最终化失败（含校验不通过）
    FileFinalizeFailed { file_name: String },
    /// 通道 C：超过保留期未恢复，被回收
    SessionExpired { retention_days: u32 },
}
```

- **存量数据兼容**：解析失败 → 当作 legacy 中文串，走兜底文案。不写迁移、不回填。
- **切语言立即生效**：存的是码，渲染时才出文案——这是方案 C 做不到的。
- 顺带**删掉 `friendlyTransferError` 那 9 条死正则**，换成 `code` 的 exhaustive 映射。

通道 D 稍有不同，因为 title 要进搜索索引。两种做法，倾向第一种：

1. **title 列只存「可渲染的事实」**：`first_file_name` + `file_count` 已经都在关联的
   files 里了，title 列可以退化成派生字段，由三端渲染。搜索索引改为索引**文件名**
   （本来单文件条目的 title 就是文件名，多文件的「等 N 个文件」几乎没有搜索价值）。
2. title 存判别码 JSON，搜索索引存渲染后的当前 locale 版本——**不推荐**，它把
   「索引内容依赖写入时的 locale」这个坑正式写进 schema。

### 方案 C：Rust 侧 i18n（rust-i18n 扩到 core）—— 否决

让 Rust 直接产出目标语言的串。四条理由，最后一条是决定性的：

1. **Rust 要知道当前 locale。** 桌面有 `set_locale` 命令，Web 与移动端没有等价物，
   要新建三份接线。
2. **`crates/transfer` 是 platform-neutral 的**，塞进全局 locale 状态与它的定位冲突
   （它连 core 都不依赖）。
3. **落库的串会冻结在写入时的 locale**（通道 D 尤其明显）——用户切语言后历史条目不变。
4. **它与刚建立的契约分叉。** 同一个传输详情页上，命令错误走 `kind` → Lingui，
   失败原因走 Rust 直出——两套机制、两处改文案的地方、两种翻译流程。
   上一轮花整个 change 收敛的就是这个，不该马上再开一条。

## 建议的范围与顺序

三件事**彼此独立**，可分开做，按收益排序：

| # | 事项 | 规模 | 收益 |
|---|---|---|---|
| 1 | 通道 B + C 判别码化，删 `friendlyTransferError` 的正则 | 2 个构造点 + 1 个 enum + 三端文案表各加 2 条 | **顺带修一个真 bug**：文件名参与错误匹配，校验失败会被显示成「传输已取消」 |
| 2 | 通道 D：title 退化为派生 + 搜索索引改索引文件名 | 2 个存储实现 + 三端列表渲染 | 收件箱条目跟随语言；消掉唯一的存量数据问题 |
| 3 | 通道 G：`crates/webrtc-p2p` 的 19 处 `#[error]` 改英文 | 纯文本替换 | subtree split 前必须做，越晚越贵 |

**通道 F（MCP 工具描述）建议不动。** 它的受众是 AI agent，不是人；agent 读中文没有障碍，
而 MCP 客户端也没有 locale 协商的概念。真要改也是「统一成英文」，与 i18n 无关。

## 一条可留的判据

判断一个 Rust 串要不要 i18n，问两句——**与 `AppError` 加 kind 的判据是同一套**：

1. **它会原样出现在用户眼前吗？** 日志、panic、测试、给 agent 的描述都不会。
2. **它跨过存储或进程边界了吗？** 跨过就不能存渲染结果，只能存判别码——
   否则它会冻结在写入时的 locale。

两问都「是」→ 判别码化。第一问「否」→ 保持中文，那是开发者语言。
