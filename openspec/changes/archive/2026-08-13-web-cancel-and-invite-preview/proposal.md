## Why

「Web 端 ↔ 桌面端能力对照」（2026-07-30）拆出的 #98 / #99 都不是架构问题 ——
**域能力早就在 `crates/transfer` / `crates/invite` 里，缺的只是 `crates/web` 的一层导出**。
它们被排在三端抽象层重构（C2–C6）之前，正因为它们一条根因都不命中：不改 trait、不改
storage、不碰端口层。

**#99 传输取消 —— `WebNode` 有 accept / reject / resume / send，唯独没有 cancel。**
域侧两条能力都在，且已经把 issue 列的三条硬约束做完了：

| issue #99 的硬约束 | 域层现状 |
|---|---|
| 按 wire 通知对端，不能只抹本地 UI | `flow/send.rs:319 notify_cancel` / `flow/receive.rs:259` 同款 |
| 走既有 projection 通路进终态 | `coordinator.dispatch(UserCommand::Cancel)`（`send.rs:322` / `receive.rs:262`） |
| 取消后不能复活成「可续传」 | `coordinator.rs:467 user_cancel_to_terminal_not_recoverable` 已钉 `recoverable == false` |

`cancel_send` 还额外处理「offer 已发出、对方未接受」这条边界（`send.rs:302-318`：没有
send actor 时回落到 `outbound_offers`，标 `cancelled_outbound_offers` 并丢 prepared）。
桌面的 `cancel_send` / `cancel_receive` 命令（`src-tauri/src/commands/transfer.rs:173` / `:183`）
就是两行薄壳，Web 侧要写的是同样的两行。

**但 issue 的第四条硬约束在 Web 侧真的没做完**：「接收侧取消后，OPFS 上的半成品要一并清理」。
`cancel_receive` 确实调了 `cleanup_part_files`（`receive.rs:260` → `actor/receiver.rs:662`），
它走 `FileAccess::cleanup_sink` 端口 —— 而 **Web 的实现只丢句柄、不删文件**：

```rust
// crates/web/src/file_access.rs:155
async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
    self.sinks.borrow_mut().remove(sink);   // ← 全部
    Ok(())
}
```

端口在 `crates/host/src/ports.rs:189` 给的是**默认 no-op**，桌面靠 `part_file.cleanup()` 自己履约，
Web 继承了 no-op 的语义。`crates/web/README.md:133` 早把它记成已知负债。雪上加霜的是
Web 侧**没有 `.part` 中间态**（`file_access.rs:99/111` 直接开 `relative_path` 的写句柄），
所以残留是一个「文件名对、内容截断」的东西躺在 OPFS 里，比桌面的 `.part` 更容易被误当成完整文件。

**#98 配对前无预览 —— 这是对照里唯一的行为安全差异。** 桌面把消费邀请拆成两步
（`decode_pair_invite`，`src-tauri/src/commands/pairing.rs:50`：纯本地解码验签、零出网），
Web 没有对应导出，用户在 `connect_invite`（`_components/pairing-panel.tsx:89`）成功之前
看不到自己在跟谁配对。配对一旦成功就是长期信任（PRODUCT.md 原则 1），而 Web 端目前还
**没有解除配对的路径**（→ #100 / C4），误配代价更高。

缺 decode 还连带卡着另外两件事，代码里已经标了：

```
// pairing-panel.tsx:194-199
// 与桌面端 `ClipboardInviteBanner` 的两处差距，都是缺 decode 能力所致（#98 会补上）：
//   1. 自我过滤只能比对**当前**这条生成的串。历史生成的、或隔壁标签页生成的漏得掉；
//      桌面端的判据是解码后 `preview.peerId === selfPeerId`，结构性的。
//   2. 没有「已配对过滤」——配完之后剪贴板里那条邀请还在，这里仍会填进输入框。
```

**顺带一条移动端的同构缺口。** `mobile-core/src/transfer.rs:244 cancel_transfer` 把两个方向
合并成一个入口，靠**先试 `cancel_send`、失败再试 `cancel_receive`** 猜方向（`:247-255`），
两条错误串拼起来返回。桌面是两条独立命令、由调用方按 `direction` 分支
（`src/lib/transfer-actions.ts:29-33`）。试错法有真实的误触发面：`cancel_send` 因**任何**
原因失败（不只是「会话不存在」）都会接着去打接收侧。`pause_transfer:259` 同款。
Web 侧现在正好要新造两条导出 —— 与其新增一个「Web 学移动端猜方向」的第三种做法，
不如趁这次把移动端拉回桌面的形状，三端一致。

## What Changes

- **`crates/web` 新增两条取消导出**：`cancel_send(session_id)` / `cancel_receive(session_id)`，
  各自直调 `TransferManager` 的同名方法，形状与 `reject_offer`（`node.rs:612`）逐字对齐。
  **不补任何域逻辑** —— 域层已完备，见上表。
- **`OpfsFileAccess::cleanup_sink` 改成真删 OPFS 条目**（`crates/web/src/file_access.rs:155`）。
  这是 #99 验收标准「接收方不会留下半成品」的兑现处。**`FileAccess` trait 一个字不动**，
  改的是 Web 那一份实现 —— 桌面早就是删的，这次是 Web 追上，不是端口重新定义（见 design D3）。
- **`crates/web` 新增 `decode_invite_preview(invite)`**：`swarmdrop_invite::PairInvite::decode`
  已内含验签（`invite.rs:257` 从 `inviter_id` 就地恢复公钥比对签名），返回新 DTO
  `PairInvitePreviewJson`（peerId / displayName / displayPlatform / expiresAt / localOnly），
  对齐桌面 `PairInvitePreview`。**纯本地，不拨号、不查 DHT。**
- **Web 前端：邀请预览确认卡。** 粘贴、剪贴板 paste 感知、`/p/` 落地页 handoff
  （`docs/public/p/index.html:234` 经 sessionStorage / `:238` 经 fragment）三条入口
  **共用同一道确认** —— 「用户点了链接」不算已确认。确认前零出网；取消 = 邀请不被消费，仍可再用。
  顺带兑现注释里欠的两件事：自我过滤换成结构性判据 `preview.peerId === 本机 nodeId`、
  新增「这台设备已经配过了」的识别。
- **Web 前端：传输取消入口。** `_components/transfer-activity-panel.tsx` 的展开区
  （续传按钮旁）为非终态会话加取消，按 `projection.direction` 分派到两条导出之一。
- **移动端 `cancel_transfer` / `pause_transfer` 拆回两条 uniffi 导出**，消掉 `:247-255` 与
  `:262-270` 的试错猜方向。RN 两处调用点跟着改：`transfer/[sessionId].tsx` 有现成的
  `projectionDirection(projection)`；`core/foreground-service.ts` 的通知 `data`（`:173`）
  目前只带 `{ kind, sessionId }`，需要把已有的 `p.direction`（`:166` 在用）一起写进去。

## Capabilities

### New Capabilities

- `web-invite-preview`: Web 端在发起配对握手前，本地解码并验签邀请、展示对端身份与剩余有效期，
  用户确认后才出网；本机自己的邀请与已配对设备的邀请被结构性识别出来。

### Modified Capabilities

- `transfer-cancel-controls`: 取消 / 暂停在三端统一为**方向显式**的两条能力。Web 端从
  「没有取消」补齐为收发双向可取消（含 OPFS 半成品的真实清理），移动端从「试错猜方向」
  改为调用方按 `direction` 分派。

## Impact

- **`crates/web`**：`node.rs` +3 个 `#[wasm_bindgen]` 方法与 1 个 `typescript_type` 包装；
  `types.rs` +`PairInvitePreviewJson`；`lib.rs` 再导出；`tests/specta_export.rs` 注册新类型并
  重生成 `bindings/bindings.ts`；`file_access.rs` 的 `cleanup_sink` 换实现；`opfs.rs` +删除原语。
  新增依赖：`swarmdrop-invite`（工作区内、wasm-clean，见 design D9）。
- **`docs/`**：`_components/pairing-panel.tsx`（确认卡两态 + 三入口收口）、
  `_components/transfer-activity-panel.tsx`（取消入口）、`_lib/view-types.ts`（再导出新类型）、
  `_lib/store.ts`（若确认态需要跨组件；见 design D6 倾向不进 store）。
  **`docs/packages/swarmdrop-web/` 是入库的生成物**（`git ls-files` 可见），改完 `crates/web`
  必须 `pnpm build:wasm` 重新生成并一起提交，否则前端拿到的是旧 `.d.ts`。
- **`mobile/`**：`mobile-core/src/transfer.rs` 拆两条导出；
  `packages/swarmdrop-core/src/generated/*` 由 `ubrn build ios|android --and-generate` 重生成
  （checksum 会变，`swarmdrop_mobile_core.ts:7091/7157` 那两处断言跟着更新）；
  `src/app/transfer/[sessionId].tsx` 与 `src/core/foreground-service.ts` 两处调用点。
- **`crates/transfer` / `crates/host` / `crates/core`：零改动。**
- **回归**：Web 侧收 / 发双向取消各一次并核对对端也进终态；取消后刷新页面确认会话仍是终态
  且不可续传；移动端前台服务通知上的暂停 / 取消按钮在收发两个方向各点一次
  （**这是拆导出后最容易漏的一处** —— 它不在页面里，direction 是新塞进通知 payload 的）。

**风险**：

1. **`cleanup_sink` 从 no-op 变成真删。** 它有第二个调用方 —— `crates/transfer/src/lib.rs:79`
   的 `cleanup_expired_part_files`（先 `open_or_create_sink` 再 `cleanup_sink`）。Web 当前不走
   这条（只有 `mobile-core/src/history.rs:224` 与 core 的 e2e 测试调它），但改完之后它对 Web
   就是「能正确工作」而不是「悄悄什么都没做」。误删风险已由域层排除：`receiver.rs:592`
   在 `finalize_sink` 成功后立刻 `remove_created_sink`，已完成的文件不在 `cleanup_part_files`
   的清单里 —— 但这条不变量值得在实施时复核一遍，它现在是「no-op 掩盖了一切」的状态。
2. **OPFS 的写句柄持有独占锁。** 未 `abort()` 就 `remove_entry` 会撞
   `NoModificationAllowedError`，且失败是异步的、只落一句 `warn!`。顺序必须是先 abort 再删。
3. **移动端 uniffi 契约变更**需要重新 `ubrn build`；这一步在 CI 里没有门禁
   （`rust.yml` 只跑 workspace 的 cargo，不跑 ubrn），漏做的表现是运行时
   `ApiChecksumMismatch`，而不是编译错误。

**非目标**：

- **任何 trait 改动、任何 storage 层改动。** 具体地：`crates/web/README.md:139-141` 提议的
  「给 `FileAccess` 加一条显式的『丢弃部分产物』方法，顺带把 `src-tauri` 直接
  `tokio::fs::remove_file` 的绕行收编回来」**不在本 change 内** —— 那是端口契约的事，
  三端一起动。本 change 只让 Web 那一份实现兑现桌面早已在兑现的语义。
- **删除传输历史记录**（→ C2 `transfer-store-port-completion`）。取消 ≠ 删除：取消后的会话
  仍留在历史里显示为「已取消」。
- **Web 端解除配对**（→ C4 `atomic-unpair-and-paired-device-store`）。#98 正文提到它是
  「误配代价更高」的**理由**，不是本 change 的交付物。
- **发送方向的跨刷新续传**：浏览器无法在用户不重新选择的前提下再读同一个 `File`，
  非终态发送会话本就不落库（`crates/web/src/store.rs:256 worth_persisting`）。
- **本地判定邀请是否被撤销**：撤销状态只在邀请方的注册表里，受邀方纯本地解码看不到（design D5）。
