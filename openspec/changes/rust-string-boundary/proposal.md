## Why

调研见 [`dev-notes/research/2026-08-rust-side-user-strings.md`](../../../dev-notes/research/2026-08-rust-side-user-strings.md)。

全仓 1466 处中文字符串字面量里，真正会**原样出现在用户眼前**的只有 3 条通道、3 个函数。
其余是日志、`expect`、测试数据、给 AI agent 的 MCP 描述 —— 那些的受众本来就是开发者或机器。
`AppError.message` 已在 `failure-semantics-contract`（`81c99617`）关掉，三端只吃 `kind`。

剩下三条：

### ① 会话失败原因 `session.error_message`

`ActorReport::FatalError(String)` 全仓只有一个构造点，消息是
`format!("文件最终化失败: {} (file_id={}): {}", file_info.name, …, e)`，落进 `error_message`
列后被**三端 6 个组件直接渲染**。

**移动端那层「对策」是个现存 bug。** `friendlyTransferError` 对整条消息跑 9 条英文关键词
正则 —— 而**文件名就拼在消息里**：

| 文件叫 | 命中 | 显示 | 真实原因 |
|---|---|---|---|
| `Q3-cancel.xlsx` | `/(cancel\|abort)/` | 「传输已取消」 | 校验失败 |
| `network-diagram.png` | `/(network\|connect…)/` | 「网络连接中断」 | 校验失败 |

确定性复现。「传输已取消」把一次**数据损坏**说成用户自己的操作。

### ② 会话过期回收

`expired_receive_reason()` → `"会话超过 N 天未恢复，已过期回收"`，同一列同一条渲染路径。

### ③ 收件箱条目标题

`inbox_title()` 返回 `"空传输"` / `"{first} 等 {n} 个文件"`，**落库**到 `inbox_items.title`
并进 `inbox_search_index.title`。存量条目已经是中文，切语言不变。

### ④ `crates/webrtc-p2p` 的中文 `#[error]`（性质不同）

按 CLAUDE.md 它「刻意不依赖任何 swarmdrop crate，将来要 subtree split 出去独立发布」。
一个准备发到 crates.io 的通用 libp2p transport，`Display` 与公开 doc 是中文 —— 读者是
Rust 生态的其他开发者。**这不是 i18n 问题，是开源库的语言约定问题**，正确答案是英文。

## What Changes

- **新增判别码 `FailureCode`**（`crates/transfer/src/failure.rs`），取代 `error_message` 的
  自由文本。`error_message` 列**不改类型**（仍是 TEXT），改存 JSON；解析失败的存量行
  归入 `FailureCode::Legacy { message }`，不写回填、不双写。
- **删掉 `friendlyTransferError` 的 9 条正则**，换成 `code` 的穷尽映射 —— 顺带修掉上面那个
  「文件名参与错误匹配」的 bug。桌面与 Web 从裸渲染改为按 code 出文案。
- **`inbox_title` 语义改为「首个文件名」**（0 文件返回空串），三端按
  `item_count` 的 0 / 1 / N 三分支渲染。**加一条一次性回填迁移**把存量 title 重算为首文件名
  —— 不回填会渲染成「X 等 3 个文件 等 3 个文件」，而识别旧格式又要再引入一次自由文本匹配。
- **`crates/webrtc-p2p` 的 `#[error]` 与公开 API doc 注释改英文**（内部注释、日志、测试不动）。

## 明确不在范围内

**MCP 工具描述与错误不动**（`src-tauri/src/mcp/tools.rs`，~97 处）。受众是 AI agent 不是人，
MCP 协议也没有 locale 协商；agent 对中英文描述的理解力相同。改它与 i18n 无关。

**日志 / `expect` / 测试数据不动。** 那是开发者语言，翻译它们只会让搜 issue 变难。
