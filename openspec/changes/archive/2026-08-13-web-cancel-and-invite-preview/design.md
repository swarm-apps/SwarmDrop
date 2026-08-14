# web-cancel-and-invite-preview 设计

三端抽象层重构（C1–C6）里的第一个，也是唯一一个**零架构风险**的。
#98 与 #99 一条根因都不命中（trait 覆盖不全 / 端口无出口 / 端口层缺域），
所以它不该被重构挡住，也不该等 C2 的 `store()` accessor。

排序判据写在这里，免得后来的人以为顺序是随手排的：本 change 触碰的每一处
要么是新增的 `#[wasm_bindgen]` 导出，要么是**单个 host 实现内部**的替换。
`crates/transfer` / `crates/host` / `crates/core` 的公开面一个字节都不变。

---

## D1：cancel 只补导出，不补域逻辑 —— 先证明再动手

issue #99 明写「域侧能力应当已在 `crates/transfer` 里，**先确认再决定是补导出还是补域逻辑**」。
确认结果：**全在**，逐条对上。

| #99 的硬约束 / 验收 | 域层实现 | 位置 |
|---|---|---|
| 按 wire 通知对端 | `notify_cancel(session.peer_id, sid)` | `flow/send.rs:319`、`flow/receive.rs:259` |
| 走既有 projection 通路进终态 | `coordinator.dispatch(User(Cancel))` | `flow/send.rs:322`、`flow/receive.rs:262` |
| 不复活成「可续传」 | `recoverable = false` | `coordinator.rs:98/110`，测试 `:467` |
| 不影响其它会话 | 全部按 `session_id` 索引 actor / offer 表 | `remove_send_actor` / `get_receive_actor` |
| 接收侧清半成品 | `session.cleanup_part_files()` | `flow/receive.rs:260` → `actor/receiver.rs:662` |

外加一条 issue 没列、但用户一定会撞上的边界：**offer 已发出、对方还没接受时的取消**。
`cancel_send` 在没有 send actor 时不是直接报错，而是回落到 `outbound_offers`
（`send.rs:302-318`）：记 `cancelled_outbound_offers`、丢弃 `prepared`、照样 dispatch `Cancel`。
Web 端「发出去等半天对方不理」正是最需要止损的场景，白捡。

所以本 change 的 Rust 侧就是两个六行方法。**任何「要不要在 Web 侧补一点取消逻辑」的念头
都应当先回来看这张表** —— 域层不缺，缺的是一条 wasm 边界上的线。

**选项与取舍**：
- （A）只补导出 —— **选中**。域层已完备，重复实现只会造出第二条状态机路径。
- （B）Web 侧另写一条轻量取消（只置本地终态、不通知对端）—— 否决。#99 第一条硬约束
  直接禁止：「不能只在本地把 UI 抹掉」。而且它会让 Web 成为唯一一个「取消后对端还在传」的端。

---

## D2：Web 侧导出两条，不合并成一条「自动判方向」

Web 是新写的，可以自由选形状。两个先例摆在面前：桌面两条独立命令
（`src-tauri/src/commands/transfer.rs:173` / `:183`），移动端一条 + 试错
（`mobile-core/src/transfer.rs:244`）。

**选两条。** 理由不是对称美：

1. **试错法有真实的误触发面。** `cancel_transfer:247-255` 的判据是「`cancel_send` 返回 Err」，
   而不是「返回的是『发送会话不存在』」。`cancel_send` 里 `coordinator.dispatch` 失败、
   store 写失败，一样会走进 `cancel_receive` 分支。取消是**有副作用**的操作
   （发 wire 帧、删文件、写终态），拿它当探针不合适。
2. **错误信息被拼成两条。** `format!("取消传输失败: {send_err}; {receive_err}")` 交给用户的是
   两句互相矛盾的话（「发送会话不存在」+「接收会话不存在」），真实原因在哪一句里靠猜。
3. **调用方本来就知道方向。** Web 的 `TransferProjection` 有 `direction`
   （`transfer-activity-panel.tsx:47 DIRECTION_LABEL` 已经在用），移动端 `[sessionId].tsx:358`
   有 `projectionDirection(projection)`。省下的那个参数没有换来任何东西。

顺带把移动端拉回同一形状（D8），于是三端一致，不留「Web 学了移动端的坏形状」这种后账。

---

## D3：OPFS 半成品必须在本 change 删掉 —— 且不需要改 trait

**这一节推翻了立项时的一个判断。** 立项材料写的是「`cleanup_part_files` 走 `FileAccess` 端口
→ Web 侧即 `OpfsFileAccess::cleanup_sink`，OPFS 清理自动生效，一行都不用写」。**不成立。**

```rust
// crates/web/src/file_access.rs:155
async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
    // 移除即 drop writable 句柄；未 close 的 staging 写入被丢弃——正是取消/失败时该有的行为。
    self.sinks.borrow_mut().remove(sink);
    Ok(())
}
```

只丢句柄，**不删 OPFS 条目**。端口在 `crates/host/src/ports.rs:189` 提供的是默认 no-op 实现，
Web 等于继承了那个语义。`crates/web/README.md:133-141` 早就把它记成已知负债并写明
「Web 侧每一次取消 / 失败的接收都会在 OPFS 留下部分文件」。

而 #99 的验收标准原文是：**「发送方在传输进行中点取消 → 双方会话都进终态，接收方不会留下半成品」**。
照抄「一行都不用写」会直接漏掉一条验收。

Web 侧比桌面更糟的一点：**没有 `.part` 中间态**。`create_sink` / `open_or_create_sink`
（`file_access.rs:99` / `:111`）直接开 `relative_path` 的写句柄，写的就是最终路径。所以残留是一个
**文件名正确、内容截断**的东西 —— 桌面留下的至少还叫 `xxx.part`，一眼能看出没写完。

**选项**：

- **（A）在 `OpfsFileAccess::cleanup_sink` 内部真删 —— 选中。**
  `FileAccess` trait 签名、doc、默认实现全部不动；改的是 Web 那一份实现。桌面（`part_file.cleanup()`）
  早就是删的，这是 **Web 追平桌面**，不是端口重新定义。因此它没有越过 C1「零 trait 改动」的边界。
- （B）按 README 的建议给 `FileAccess` 加一条显式的「丢弃部分产物」方法，顺带收编
  `src-tauri` 那处绕开端口直接 `tokio::fs::remove_file` 的过期回收 —— **否决（列为非目标）**。
  它动端口契约、三端一起改、还牵扯桌面回收路径的重构，正是本 change 承诺不碰的那一类。
  README 的判断（「不是 Web 单方面偷懒，而是端口契约没写清」）依然成立，那笔账留着，
  但它不该把一条已经写在 issue 验收里的用户可见缺陷一起扣住。
- （C）什么都不做，靠浏览器站点存储配额兜底 —— 否决。配额兜底的表现是**整个站点写不进去**，
  波及正在进行的其它接收，而不是「旧残件被回收」。

**实现上必须注意的两点**（不写下来实施者一定踩）：

1. **顺序是先 abort 再删。** `createWritable()` 持有该文件的独占锁，锁在 `close()` / `abort()`
   时释放；只把 `FileSystemWritableFileStream` 从 map 里 drop 掉，锁要等 GC 才释放，时机不确定。
   在锁未释放时 `remove_entry` 会 reject（`NoModificationAllowedError`）。所以：
   `abort()`（而非 `close()` —— close 会把 staging 提交上去，正好是反的）→ await → `remove_entry`。
2. **删除要能处理多级 relative_path。** 沿用 `opfs.rs:62 opfs_file_handle` 的逐段走法，
   但 `create:false`：走到父目录句柄后调 `FileSystemDirectoryHandle::remove_entry(file_name)`。
   `web-sys` 的 `FileSystemDirectoryHandle` feature 已开（`crates/web/Cargo.toml:77`），
   不需要新 feature（`remove_entry` 无 options 重载即够，不做递归删目录）。
   **删不掉不算错误**：文件本就不存在（chunk 还没落过盘就取消）是常态，与其它实现一样
   `warn!` 后照常返回 `Ok`，不能让清理失败把取消流程一起拖失败。

**误删风险已由域层排除，但要复核**：`cleanup_part_files` 只遍历 `ReceiverActor::created_sinks`，
而 `receiver.rs:592` 在 `finalize_sink` 成功后立刻 `remove_created_sink`。所以多文件会话里
已完成的那些不在清单内，取消只删没写完的那个。这条不变量此前被 no-op 掩盖着从没被检验过，
实施时按它写一条测试。

**连带的正向效果**：`crates/transfer/src/lib.rs:70 cleanup_expired_part_files`
（先 `open_or_create_sink` 再 `cleanup_sink`）对 Web 从此是真的能工作。Web 目前不调它
（只有 `mobile-core/src/history.rs:224` 和 core 的 e2e 在调），所以本 change 不引入行为变化，
但将来 Web 接过期回收时不必再回头补这一处。

---

## D4：预览 DTO 的形状 —— 字段抄桌面，时间戳类型不抄

字段集合与桌面 `PairInvitePreview`（`src-tauri/src/commands/pairing.rs:25-34`）一致：
`peerId` / `displayName` / `displayPlatform` / `expiresAt` / `localOnly`。
不多给（capability 明文绝不出边界）、不少给（少一个 UI 就得二次解码）。

**唯一分叉是 `expiresAt` 的类型：桌面 `i64`，Web 用 `String`。**

- Web 侧 DTO 家族已有先例：`InviteListItemJson`（`types.rs:117`）的 `created_at` / `expires_at`
  都是 `String` 承载 Unix 秒，`PendingPairingJson.pending_id` 同理，注释写的理由是
  「避开 u64 → BigInt 的取回麻烦」。
- 前端的消费函数**已经按字符串写好了**：`_lib/invite.ts` 的
  `remainingSeconds(expiresAt: string, now: number)` 与 `formatRemaining(expiresAt, now)`,
  `pairing-panel.tsx:369` 正在用。预览卡的「剩余 X 分钟」要的就是这两个函数。
  换成 `number` 得先改它们或在调用处 `String(...)`，纯粹是自找。

代价是两端 DTO 形状不同名不同型。可以接受：它们是两个 shell 各自的 IPC 面，本来就没有
共享类型（桌面走 tauri-specta，Web 走 `crates/web/bindings/bindings.ts`），
唯一的一致性要求是**字段语义**，那条守住了。

**TTL 判定留在调用方**，不在 DTO 里放 `expired: bool`。`PairInvite::decode` 的 doc
（`invite.rs:235`）明说「TTL 由调用方按 `expires_at` 判定 —— 权威判定在发起端 `InviteRegistry`，
解码侧预检仅为 UX」。塞一个布尔进 DTO 等于把一个**在序列化那一刻就开始变旧**的判断固化下来，
而确认卡是会在屏幕上停留几十秒的。

---

## D5：本地判不出「已撤销」—— 明说，不要造假判据

#98 的验收写的是「过期 / **被撤销** / 格式非法的邀请在确认卡阶段就被拒」。
前后两项本地能判，**中间那项判不了**：

- 格式非法 / 伪造 / 篡改 → `PairInvite::decode` 就地拒（`invite.rs:257` 用 `inviter_id`
  恢复公钥验签；`extract_payload` 认不出前缀也在这里失败）。
- 已过期 → `expires_at` 与本地时钟比对。
- **已撤销 → 状态只在邀请方的 `InviteRegistry` 里**（`revoke_invite` / `revoke_invite_by_hash`
  写的是发起端的注册表与 IndexedDB）。受邀方手上只有一段自包含的签名串，撤销这件事根本没有
  传播到它这里。要判就得出网问 —— 而 #98 的硬约束第一条是「确认卡出现之前不应有任何出网行为」。

**结论**：撤销只能在 `connect_invite` 阶段由邀请方拒绝，前端把那个失败**渲染成人话**
（「这条邀请已被对方撤销或已被使用」），而不是在预览阶段假装能判。

写下来是因为不写的话，实施者面对验收清单会去发明一个本地判据 —— 最可能的发明是
「查本机 `list_invites` 看在不在」，那是**本机自己发出的**邀请列表，对受邀方永远为空，
于是所有邀请都被判成「已撤销」。

---

## D6：确认卡就地两态，不做模态；三条入口共用同一道闸

**呈现形态：就地两态。** `pairing-panel.tsx` 的「消费邀请」区块，从
`输入框 + 配对按钮` 换成 `输入框 → (解码成功) → 确认卡（设备名 / 平台 / 剩余有效期 / LocalOnly 标记）
+ 配对 / 取消`。

- **不做模态**：`pair-deep-link/design.md` D5 已经为剪贴板路径定过基调（非模态 + 点击才发起），
  而 Web 端这里比桌面更轻 —— 用户是自己动手粘贴 / 点链接进来的，本来就在等这一步，
  一个盖住页面的对话框只是多一次关闭动作。桌面之所以是模态确认卡，是因为它有从**任意路由**
  弹出来的剪贴板横幅要接。
- **状态放组件本地 `useState`，不进 store。** `_lib/create-store.ts` 是自研 store，
  「selector 里派生新数组 / 对象 → 无限重渲染」的陷阱与 zustand 同款，**而
  `pnpm check:zustand-access` 只扫仓库根 `src/`，不覆盖 docs/**（CLAUDE.md 的 Web 端硬约束 3）。
  预览态只有 `PairingPanel` 一个消费者，没有理由去冒那个没有机器兜底的风险。

**三条入口必须收口到同一个函数**，这是本节真正的要求：

| 入口 | 当前落点 |
|---|---|
| 用户手打 / 手动粘进输入框 | `pairing-panel.tsx:271 onChange` |
| 全局 `paste` 感知 | `:200-222` 的 `onPaste`，命中后 `setInviteInput(link)` |
| `/p/` 落地页 handoff | `:56-82` 的 effect，sessionStorage（`docs/public/p/index.html:234`）或 fragment（`:238`） |

#98 硬约束原文：「不要因为『用户点了链接』就当作已确认」。三条都只是**把串放进输入框**，
真正的动作是解码 → 确认卡 → 用户点确认。做法上就是让「设置邀请串」这一步统一触发解码，
而不是在三处各写一遍。

**取消语义**：清空输入框与预览态，**不调任何后端**。邀请是一次性凭证，只有
`connect_invite` 走通 capability 握手才会被邀请方 CAS 消费 —— 用户取消时它一个字节都没出网，
自然仍可再用。这是「零出网」的直接推论，不需要额外机制。

---

## D7：自我过滤与已配对过滤 —— 兑现 `pairing-panel.tsx:194` 欠的两笔

有了 `decode_invite_preview`，注释里标着「#98 会补上」的两处一起还掉。

**自我过滤**：判据从 `link === generatedInvite`（`:207`，字符串比对**当前**这一条）
换成 `preview.peerId === node.node_id()`。差别是结构性的：

- 现判据漏三种情况：历史生成的邀请、隔壁标签页生成的、刷新页面后 `generatedInvite` 已是 `null`。
- 新判据比的是 **wire 里签名覆盖范围内的 `inviter_id`**（`invite.rs:253`），伪造不了，
  与桌面 `pair-deep-link/design.md` D4 用的是同一条判据。

**已配对过滤**：`preview.peerId` 命中 `paired_devices()` 里已有的 `peerId` 时，
不亮确认卡，直接说「这台设备已经配过了」。现状是照样填进输入框、用户点了才被后端拒
（`pairing-panel.tsx:197-198` 自己承认「不算错，但桌面端是不打扰的」）。

两条都发生在**解码之后、出网之前**，不违反「确认卡出现前零出网」—— `node_id()`
（`node.rs:257`）与 `paired_devices()`（`:441`）都是同步的本地读。

---

## D8：移动端拆两条，顺带补上通知里缺的 `direction`

`mobile-core/src/transfer.rs` 的 `cancel_transfer:244` / `pause_transfer:259` 各拆成
`cancel_send` / `cancel_receive` / `pause_send` / `pause_receive`，直调
`TransferManager` 同名方法，与桌面命令一一对应。

两处 RN 调用点的处境不同，**第二处是这条任务真正的活**：

- `src/app/transfer/[sessionId].tsx:114/126` —— 页面里有 projection，
  `projectionDirection(projection)`（`core/transfer-types.ts:54`）现成，直接分支。
- `src/core/foreground-service.ts:82-92` —— **拿不到方向**。它是前台服务通知的 action 回调，
  信息全部来自 `event.detail.notification?.data`，而那份 data 在 `:173` 只写了
  `{ kind: "transfer-progress", sessionId }`。方向其实就在手边（`:166` 的 `p.direction` 正在
  用来选「发送中 / 接收中」文案），塞进 `data` 即可。兜底路径 `activeSessionId`（`:54`）
  同样要配一个 `activeDirection`。

**这是拆导出后最容易漏的地方** —— 它不在任何页面里，改完 `[sessionId].tsx` 会觉得任务完成了，
而 Android 前台服务通知上的暂停 / 取消按钮会在运行时静默失效或报参数错。

**契约变更的连带成本**：uniffi 方法名变了，`packages/swarmdrop-core/src/generated/` 必须
`ubrn build ios --and-generate` / `build android --and-generate` 重生成
（`swarmdrop_mobile_core.ts:7091` / `:7157` 的 checksum 断言会跟着变）。这一步**CI 没有门禁**
（`rust.yml` 只跑 workspace cargo），漏做的表现是运行时 `ApiChecksumMismatch` 而非编译失败。

**为什么不留 `cancel_transfer` 作兼容包装**：本次重构序列的前提是「不考虑向后兼容性」，
而留一个包装意味着试错逻辑还在代码里，下一个人照抄的概率不为零。移动端的 JS 与 Rust
在同一个仓、同一次构建里出去，没有需要兼容的外部调用方。

---

## D9：wasm 三条硬约束的核对

| 硬约束 | 本 change 是否触碰 | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **否** | `crates/core` 零改动 |
| **`crates/transfer` 零 network 依赖** | **否** | `crates/transfer` 零改动。取消能力全部复用现有 `TransferManager` 方法 |
| **`crates/invite` 零 core 依赖** | **否** | `crates/invite` 零改动。是 `crates/web` **依赖 invite**（方向朝下），不是反过来 |

补充三条本 change 自己要守的：

1. **依赖零新增。** `crates/web/Cargo.toml:22` 已有 `swarmdrop-invite = { workspace = true }`，
   `node.rs:24` 在用 `TransportPolicy`、`:358` 在用 `invite_qr_svg`。`decode_invite_preview`
   不引入任何新边。invite 本身是 wasm-clean 的（`check-wasm.sh` 的 `CRATES` 数组里就有
   `-p swarmdrop-invite`）。
2. **DTO 不吃任何 sea-orm 关系类型。** `PairInvitePreviewJson` 是五个 `String` / `bool`，
   与 `entity::*` 无关。`types.rs` 本身不受 `wasm_browser` 门控（native 也编，specta 导出
   test 在 native 注册它们），加类型时保持这一点。
3. **`./scripts/check-wasm.sh`（含 `--clippy`）必过。** 本 change 改了 `crates/web`，
   命中 CLAUDE.md 的门禁条件。

---

## D10：三条生成链路都要重跑，且都入库

改 `crates/web` 的公开面会波及三份**入库的生成物**，漏任何一份都会让前端拿到过期契约：

```
crates/web/src/types.rs  ──(cargo test -p swarmdrop-web --features specta --test specta_export)──>
    crates/web/bindings/bindings.ts        ← node.rs:55 用 include_str! 注入 .d.ts

crates/web/src/node.rs   ──(cd docs && pnpm build:wasm  →  wasm-pack build --target web)──>
    docs/packages/swarmdrop-web/{swarmdrop_web.js, .d.ts, _bg.wasm}   ← git ls-files 可见

docs/app/app/_lib/view-types.ts  ← 手工再导出新类型（它刻意不手写镜像，只 re-export）
```

新 DTO 必须在 `tests/specta_export.rs:56-64` 的 `Types::default().register::<..>()` 链上
显式注册 —— 那里不是自动扫描，漏注册的表现是 `bindings.ts` 里没有该类型、
`node.rs` 的 `#[wasm_bindgen(typescript_type = "PairInvitePreviewJson")]` 引用到一个不存在的名字。

`node.rs` 返回具名类型的既有范式（`ConnectionJsonJs` 等，`:57-86` 那个 `extern "C"` 块 +
`to_js_typed` 辅助）照抄即可，不要直接返回 `JsValue`（会在 `.d.ts` 里退化成 `any`）。
