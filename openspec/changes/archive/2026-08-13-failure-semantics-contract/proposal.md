## Why

上一次改动（`fix(pairing)` @ `f59f150b`）修好的是**一个实例**：`AppError::Identity` 当垃圾桶用，
于是「点接受配对」报「初始化设备身份失败」；以及「配对已达成但落盘失败」被 `?` 冒泡成
一次纯失败，导致两台设备对同一件事的认知永久分叉。

修的时候顺手核了一圈，发现**同一个病在别处原样存在**，只是没人报过 bug。它们不是「以后可以
优化」，是同一条规则的其余违例：

### ① 传输接受路径与配对接受路径**完全同型**

```rust
// crates/transfer/src/flow/receive.rs:168-207（本 change 之前）
let (_, offer) = self.pending.remove(session_id).ok_or(...)?;   // ← 挂起 offer 已消费
self.store.update_session_save_path(...).await?;                //   失败 = offer 白丢了
self.start_receive_actor(...);
let _ = offer.responder.send(OfferResult { accepted: true });   // ← 越线：对端开始推数据
self.coordinator.dispatch(Accept).await?;                       // ← 越线**之后**还在 ?
```

比配对那条更糟，因为它有两处越线：

- **`dispatch(Accept)` 失败** → 调用方收到 `Err`，UI 弹「接收失败」，可对端已经在推数据、
  ReceiverActor 已在落盘。用户看到失败、文件却在往硬盘里写。
- **`update_session_save_path` 失败** → offer 已被 `remove`，`responder` 随 `offer` 一起 drop，
  对端的 RPC 直接断。用户想重试都没得点 —— 那条 offer 在 UI 上已经消失了。

### ② `AppError::Transfer` 是新的垃圾桶，且已经比 `Identity` 大得多

`Identity` 出事时承载 8 处。`Transfer` 今天是 **104 处**，散在 26 个文件里，从
「收件箱条目不存在」到「navigator.storage 不可达」到「bao 逐块验签失败」全归它。
前端能给的文案只有一句 `文件传输失败，请重试` —— 对「磁盘满了」和「客户端版本不兼容」
都是错的建议。

### ③ 移动端**根本没有** kind → 文案表

桌面有 `src/lib/errors.ts`、Web 有 `WEB_ERROR_KIND_LABEL`，移动端一份都没有。

真正的规模在实施时才看清：不止 6 处 `err instanceof Error ? err.message : String(err)`，
还有 `lib/utils.ts` 里的 `errorMessage()` —— 它**专门**把 uniffi 的 `inner` 展开成人话，
然后被 **20 处 `toast.error` 当用户文案用**：

```
FfiError.Transfer: 收件箱条目不存在
```

英文界面上弹的就是这个字符串。上一次给 `AppError` 加的 4 个 kind，在移动端一个都没有对应文案。

### ④ Web 把「对方拒绝配对」渲染成「网络错误」

```rust
// crates/web/src/node.rs:489
return Err(WebError::network("邀请方拒绝了配对或配对未成功").into());
```

`PairingResponse::Refused { reason }` 是**结构化**的，桌面 `pairing-store.ts:66` 正确地按
`reason.type` 出文案。Web 把它压成一个 `network` kind + 一句写死的简体中文：用户看到的是
标题「网络错误」配一句中文正文（英文界面下尤其突兀），而真实原因是对方点了拒绝 ——
一个网络完全正常的场景。

### ⑤ `parse_hex32` 有 5 份拷贝，其中 1 份刚被发现会 panic

`crates/web/src/node.rs` / `crates/web/src/invite_store.rs` / `crates/storage-sql/src/invite.rs` /
`src-tauri/src/commands/pairing.rs` / `mobile-core/src/pairing.rs`，编码侧同样 5 份。
它们全都在处理同一个东西 —— `swarmdrop_invite` 的 `capability_hash: [u8; 32]`。
上一轮在**其中一份**里发现了多字节输入 panic（`&text[a..b]` 按字节切），修的也只有那一份。

## What Changes

- **一条明确的「越线」规则**（design D1），并按它修 `accept_and_start_receive` 与
  `reject_and_respond`：能挪的可失败步骤全部挪到越线之前，越线前的失败要**可重试**
  （offer 放回 pending），应答通道已关闭走回滚。
- **`AppError` 补两个有用户语义的 kind**：`SessionNotFound` / `StorageFailed`，
  迁移命令能直接收到的那些调用点；`Transfer` 降级为「其余传输失败」并写死判据 doc。
- **移动端补 `src/lib/errors.ts`** 并替换掉 `utils.ts` 的 `errorMessage`：26 个展示点全部
  改走 kind → Lingui 描述符，Rust 原串只进 console。
- **Web `connect_invite` 保留结构化拒绝原因**，返回值带 `refused`，前端按 reason 出文案。
- **hex 编解码收进 `crates/invite`**（`capability_hash_to_hex` / `_from_hex`），5 份拷贝删掉，
  panic 修复对全部调用点生效。

两个中途砍掉的设计（理由见 design D2 / D3，都是同一种病）：`AcceptOutcome { recorded }` ——
把 dispatch 挪到越线之前后，那个降级位恒为真；`IntegrityFailed` —— 内容校验失败走的是
`error_message: String` 通道，永远到不了 `kind`。**先设计机制、没验证它到不到得了用户。**

## 明确不在范围内

**`TransferFailedEvent.error: String` 这条通道不动。** 会话级失败原因（bao 验签失败、
源文件变更、对端中止）走的是**另一条链路** —— 不是命令返回值，而是事件 + `session.error_message`
落库，三端 UI 直接渲染那个 String。它同样是「Rust 中文串直达 UI」，跟本 change 的命令
返回路径没有共用代码。**单独立项**，不塞进来。

> **勘误（2026-08-05）**：上句原写作「改它要动 **wire**、DB 列、恢复逻辑与三端详情页」。
> **wire 那条是错的** —— `error_message` 不进跨端协议
> （`grep error_message crates/transfer/src/wire/ crates/transfer/src/protocol.rs` 零命中），
> 它是纯本地的 DB 列 + 本地事件。真实成本比这里估的低。
> 完整通道分析见 [`dev-notes/research/2026-08-rust-side-user-strings.md`](../../../dev-notes/research/2026-08-rust-side-user-strings.md)。

**Android 原生库不在本 change 的产物里。** `jniLibs/` 是 gitignore 的构建产物，跨平台绑定
（`src/generated/*.ts`、`cpp/generated/*`）由 `--and-generate` 一次生成、已随上次改动入库。
Android 只需本地/CI 重新编 `.so`，不是代码交付项。
