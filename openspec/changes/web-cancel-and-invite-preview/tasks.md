# web-cancel-and-invite-preview 任务分解

> 解锁 #99（传输取消）与 #98（配对前邀请预览确认）。
> **零 trait 改动**：`crates/transfer` / `crates/host` / `crates/core` 全程不动一个字节。
> 阶段 1–2 与阶段 3 相互独立，可并行；阶段 4–5 依赖 1–3 的导出，阶段 6 完全独立。

## 1. wasm 取消导出（crates/web，Rust）

- [x] 1.1 `crates/web/src/node.rs`：在 `reject_offer`（:612）与 `resume`（:622）之间加
      `cancel_send(&self, session_id: String) -> Result<(), JsValue>` ——
      `parse_session_id` → `self.manager.cancel_send(&sid).await` → `map_err(WebError::from)`
- [x] 1.2 同处加 `cancel_receive(&self, session_id: String)`，调 `self.manager.cancel_receive(&sid)`
- [x] 1.3 两条各写 doc 注释，**点明域层已做完的三件事**（wire 通知对端 / 清半成品 /
      dispatch `UserCommand::Cancel` 进终态），并注明 `cancel_send` 覆盖「offer 已发出、
      对方未接受」这条边界（`crates/transfer/src/flow/send.rs:302-318`）——
      免得下一个人以为 Web 侧还要补什么
- [x] 1.4 **不加** `cancel_transfer` 这类自动判方向的合并入口（design D2）

## 2. wasm 邀请预览导出（crates/web，Rust）

- [x] 2.1 `crates/web/src/types.rs` 新增 `PairInvitePreviewJson`：
      `peer_id: String` / `display_name: String` / `display_platform: String` /
      `expires_at: String`（Unix 秒，字符串承载 —— design D4）/ `local_only: bool`。
      加 `#[derive(Serialize)]` + `#[cfg_attr(feature = "specta", derive(specta::Type))]` +
      `#[serde(rename_all = "camelCase")]`，与同文件 `InviteListItemJson`（:117）保持一致
- [x] 2.2 doc 注释写明：**不含 capability**（明文绝不出边界）、**TTL 由调用方按 `expiresAt` 判**
      （不放 `expired: bool`，理由见 design D4）
- [x] 2.3 `crates/web/src/lib.rs` 的 `pub use types::{...}` 列表加上新类型
- [x] 2.4 `crates/web/tests/specta_export.rs`：`use` 列表 + `Types::default().register::<..>()`
      链（:56-64）加上 `PairInvitePreviewJson`。**那里不是自动扫描**，漏注册 = bindings.ts 里没有它
- [x] 2.5 `crates/web/src/node.rs` 的 `extern "C"` 块（:57-86）加
      `#[wasm_bindgen(typescript_type = "PairInvitePreviewJson")] pub type PairInvitePreviewJs;`
- [x] 2.6 `node.rs` 加 `decode_invite_preview(&self, invite: String) -> Result<PairInvitePreviewJs, JsValue>`：
      `swarmdrop_invite::PairInvite::decode(&invite)`（已内含验签，`crates/invite/src/invite.rs:257`）
      → 映射 DTO → `to_js_typed(&preview, "邀请预览")`
- [x] 2.7 解码失败映射成 `WebError::invalid_input`（**不是** `network`）——
      「链接不对」与「网络错误」是两码事，桌面在 `pair-deep-link` 收尾时正是为这个改过分类
      （`src-tauri/src/commands/pairing.rs:52-57` 的注释）。技术细节只进 `tracing::debug!`
- [x] 2.8 doc 注释注明**纯本地**：不拨号、不查 DHT、不碰 IndexedDB；且**判不出「已撤销」**
      （design D5），撤销只能由 `connect_invite` 阶段的邀请方拒绝
- [x] 2.9 `cargo test -p swarmdrop-web --features specta --test specta_export` 重生成
      `crates/web/bindings/bindings.ts` 并**入库**（`node.rs:55` 用 `include_str!` 注入 .d.ts）

## 3. OPFS 半成品真删（crates/web，Rust）—— 兑现 #99 的第四条硬约束

> 立项材料说「OPFS 清理自动生效，一行都不用写」是**错的**：`file_access.rs:155` 只丢句柄。
> 详见 design D3。

- [x] 3.1 `crates/web/src/opfs.rs` 新增 `pub(crate) async fn remove_path(relative_path: &str) -> AppResult<bool>`：
      沿用 `opfs_file_handle`（:62）的逐段走法但 `create:false` 走到**父目录句柄**，
      再调 `FileSystemDirectoryHandle::remove_entry(file_name)`。
      `web-sys` 的 `FileSystemDirectoryHandle` feature 已开（`crates/web/Cargo.toml:77`），无需新增
- [x] 3.2 `remove_path` 对「文件不存在」返回 `Ok(false)` 而非 Err ——
      chunk 还没落过盘就取消是常态，不能让它把取消流程拖失败
- [x] 3.3 沿用本模块纪律：`!Send` 句柄绝不跨 await，只让 `SendWrapper<JsFuture>` 跨；
      加 5s 超时兜底（与 `opfs_root` / `export_blob_url` 同款，保证永不挂死）
- [x] 3.4 `crates/web/src/file_access.rs:155 cleanup_sink` 改实现，顺序**必须**是：
      从 `self.sinks` 取出 writable → `abort()`（**不是 `close()`** —— close 会把 staging
      提交上去，正好相反）→ await → 从 map 移除 → `opfs::remove_path(&sink.0)`
- [x] 3.5 `remove_path` 失败只 `warn!` 不上抛（与 `receiver.rs:666` 现有的告警语义一致）
- [x] 3.6 改写 `cleanup_sink` 的 doc 注释：说清它现在**真的删 OPFS 条目**，
      以及 Web 侧写的是**最终路径而非 `.part`**（`file_access.rs:99/111`），
      所以残件长得像一个正常文件 —— 这正是必须删的理由
- [x] 3.7 `crates/web/README.md:133-141` 那条已知负债改写：Web 侧不再留残件；
      保留「端口契约没写清 + 桌面过期回收绕开 `FileAccess`」这半条，标为**后续 change**
      （给 `FileAccess` 加显式「丢弃部分产物」方法，本 change 明确不做）
- [x] 3.8 复核域层的不变量并**写一条测试**：`receiver.rs:592` 在 `finalize_sink` 成功后
      `remove_created_sink`，所以多文件会话取消时**已完成的文件不在 `cleanup_part_files`
      清单里**。这条此前被 no-op 掩盖着从未被检验，现在它决定会不会误删用户文件
- [x] 3.9 `wasm-pack test --headless --chrome -p swarmdrop-web` 加一条 `remove_path` 的
      往返测试（建文件 → 删 → `export_blob_url` 应快速失败），与 `opfs.rs:166` 现有测试同处

## 4. Web 前端 —— 传输取消入口（docs/app/app）

- [x] 4.1 `_lib/view-types.ts` 的 `export type { ... } from "swarmdrop-web"` 加 `PairInvitePreviewJson`
- [x] 4.2 `_components/transfer-activity-panel.tsx`：新增 `cancelAction = useKeyedAsyncAction()`
      （与现有 `resumeAction`（:138）同款，按 sessionId 分键 —— 多会话可并发取消）
- [x] 4.3 加引用稳定的 `cancel` 回调（`useCallback`，依赖 `[cancelAction.run]`）——
      列表项是 `memo` 的（:213 注释），每次渲染新建回调会把 memo 打穿
- [x] 4.4 回调内按 `projection.direction` 分派 `node.cancel_send` / `node.cancel_receive`
      （不猜方向，design D2）
- [x] 4.5 `TransferActivityItem` 的 props 加 `cancelPending` / `cancelError` / `onCancel`，
      在展开区（:315-328，续传按钮那一行）为**非终态**会话渲染取消按钮 ——
      判据用 `_lib/format.ts:24 isActiveSession`，**不要另写一份**（导航徽标也用它）
- [x] 4.6 按钮加二次确认（`window.confirm` 或就地两态皆可）：取消是不可逆的终态动作，
      而它紧挨着「续传」按钮，误点代价不对称
- [x] 4.7 取消失败经 `WebErrorCard` 展示（与 `resumeError` 同处，:330）
- [x] 4.8 取消后**不做任何本地状态修改** —— 终态经 `TransferProjection` 事件从内核回流
      （`_lib/event-dispatch.ts`），前端抢着改会与回流的那份打架

## 5. Web 前端 —— 邀请预览确认卡（docs/app/app）

- [x] 5.1 `_components/pairing-panel.tsx`：新增本地态 `preview: PairInvitePreviewJson | null`
      与 `previewError`。**不进 store** —— `_lib/create-store.ts` 是自研 store，
      selector 派生对象会无限重渲染且 `pnpm check:zustand-access` 不覆盖 docs/（design D6）
- [x] 5.2 抽出 `setInviteAndPreview(link: string)` 单一收口：设置输入框 → `decode_invite_preview`
      → 成功置 `preview`、失败置 `previewError`
- [x] 5.3 三条入口全部改调它，**一条不漏**：
      输入框 `onChange`（:271）/ 全局 `paste` 监听（:200-222）/ `/p/` 落地页 handoff effect（:56-82）。
      #98 硬约束原文：「不要因为『用户点了链接』就当作已确认」
- [x] 5.4 自我过滤判据换成结构性的：`preview.peerId === node.node_id()`，替掉
      `:207` 的 `link === generatedInvite`（那条只比得了**当前**这一串，历史的 / 隔壁标签页的 /
      刷新后的全漏）。兑现 `:194-199` 注释里欠的第 1 笔
- [x] 5.5 新增已配对过滤：`preview.peerId` 命中 `paired_devices()` 时不亮确认卡，
      改说「这台设备已经配过了」。兑现 `:194-199` 欠的第 2 笔
- [x] 5.6 确认卡 UI：设备名（`displayName`）/ 平台（`displayPlatform`）/ 剩余有效期 /
      LocalOnly 标记 / 短 peerId。剩余有效期直接复用 `_lib/invite.ts` 的
      `formatRemaining(expiresAt, now)`（面板已有共享时钟 `useNowSeconds()`，:41）
- [x] 5.7 已过期的邀请（`remainingSeconds(...) <= 0`）在确认卡位置就拒，给出「已过期」
      而不是把「配对」按钮亮出来让用户白点一次
- [x] 5.8 「配对」按钮改为**只有 `preview !== null` 时可点**，点击才走 `connect_invite`
      （原 `doConsumeInvite`，:84-97）
- [x] 5.9 「取消」按钮：清空 `inviteInput` / `preview` / `previewError` / `pastedFromClipboard`，
      **不调任何后端**。邀请只在 `connect_invite` 走通 capability 握手时才被 CAS 消费，
      所以取消天然不消费（design D6）
- [x] 5.10 `connect_invite` 阶段的失败文案覆盖「已撤销 / 已被使用」——
      那是**唯一**能得知撤销状态的地方（design D5）。不要去发明本地撤销判据
- [x] 5.11 解码失败文案分情况：格式不认（前缀 / base32）、验签不过、已过期。
      三者都由 `PairInvite::decode` 的 `InviteParseError` 区分，不要拍成一句「邀请无效」
- [x] 5.12 `:194-199` 那段「#98 会补上」的注释删掉或改写成现状 ——
      留着会让下一个人以为还欠着

## 6. 移动端 —— cancel / pause 拆两条（mobile/）

- [x] 6.1 `mobile/packages/swarmdrop-core/rust/mobile-core/src/transfer.rs`：
      `cancel_transfer:244` 拆成 `cancel_send` / `cancel_receive` 两条 `#[uniffi::export]`，
      各自直调 `manager.cancel_send` / `manager.cancel_receive`
- [x] 6.2 `pause_transfer:259` 同款拆成 `pause_send` / `pause_receive`
- [x] 6.3 删掉 `:247-255` 与 `:262-270` 的试错猜方向与两条错误串拼接
      （`format!("取消传输失败: {send_err}; {receive_err}")`）
- [x] 6.4 **不留 `cancel_transfer` / `pause_transfer` 兼容包装**（design D8：
      本重构序列不考虑向后兼容，留着等于把试错逻辑留在代码里给人照抄）
- [x] 6.5 `mobile/src/app/transfer/[sessionId].tsx`：`onPause`（:114）与 `performCancel`（:126）
      按 `projectionDirection(projection)`（`core/transfer-types.ts:54`，该文件 :358 已在用）分派
- [x] 6.6 `mobile/src/core/foreground-service.ts:173`：通知 `data` 加 `direction`
      （`p.direction` 在 `:166` 已经在用来选「发送中 / 接收中」文案，就在手边）
- [x] 6.7 同文件 `:54` 的 `activeSessionId` 兜底旁加 `activeDirection`，
      在 `:164` 一并赋值、`:136` / `:191` 一并清空
- [x] 6.8 `handleForegroundServiceEvent`（:77-96）按 direction 分派到四条导出。
      **这是拆导出后最容易漏的一处** —— 它不在任何页面里，改完页面很容易以为完事了
- [x] 6.9 重新生成 uniffi bindings：`mobile/packages/swarmdrop-core` 下
      `pnpm build:ios`（`ubrn build ios --and-generate && bob build`）与 `pnpm build:android`。
      `src/generated/swarmdrop_mobile_core.ts` 的 checksum 断言（:7091 / :7157）会变，一并入库
- [x] 6.10 `mobile/` 下 `pnpm typecheck` 过

## 7. 验证与门禁

- [x] 7.1 `cargo fmt --all`
- [x] 7.2 `cargo check --workspace --all-targets`
- [x] 7.3 `cargo test --workspace`（含 `crates/transfer/src/coordinator.rs:467`
      `user_cancel_to_terminal_not_recoverable` —— 它是「取消后不复活成可续传」这条验收的机器凭据）
- [x] 7.4 `cargo clippy --workspace`
- [x] 7.5 `./scripts/check-wasm.sh`
- [x] 7.6 `./scripts/check-wasm.sh --clippy`
- [x] 7.7 `cd docs && pnpm build:wasm` 重生成 `docs/packages/swarmdrop-web/`
      （`.js` / `.d.ts` / `_bg.wasm` 三份都**入库**，`git ls-files` 可见）
- [x] 7.8 `cd docs && pnpm typecheck` + `pnpm build`（静态导出必须过）
- [x] 7.9 `pnpm check:zustand-access` —— **本 change 不碰仓库根 `src/`**，跑它只为确认这一点
      （它也只扫根 `src/`，docs/ 的自研 store 没有机器兜底，靠 5.1 的纪律）
- [x] 7.10 `pnpm test`（根 vitest，本 change 预期零影响，作回归基线）
- [x] 7.11 `wasm-pack test --headless --chrome -p swarmdrop-web`（3.9 的 OPFS 删除测试）

### 手动验收 —— #99

- [ ] 7.12 Web 发送方在传输进行中取消 → 双方会话都进终态；接收端（桌面）**不留半成品**
- [ ] 7.13 Web 接收方在传输进行中取消 → 同上，且**浏览器 OPFS 里那个截断文件已消失**
      （DevTools → Application → Storage 逐项确认，或调 `download_url` 应失败）
- [ ] 7.14 Web 发送方在 offer 已发出、对方尚未接受时取消 → 会话进终态、对端 offer 消失
      （走 `flow/send.rs:302-318` 那条回落分支）
- [ ] 7.15 取消一条会话不影响同时进行的另一条（并发两条传输，取消其一）
- [ ] 7.16 取消后刷新页面 → 会话仍是终态「已取消」，**不出现续传按钮**
- [ ] 7.17 多文件会话传到一半取消 → **该会话已开的 OPFS 文件全部消失**，一个不留。
      **本条已按实测订正**（原文写的是「第一个文件已完成的那个仍在」，按现有架构做不到）：
      `ReceiverActor` 在 `finish_data_channel` 里**一次性 finalize 全部文件**，
      不是每收完一个就 finalize。所以传到一半取消时没有任何文件被提交，
      `created_sinks` 里就是全部已开的 sink —— 全删是正确语义（没有一个文件交付给用户，
      会话整体作废）。
      3.8 那条不变量（已 finalize 的不在清单里）**真实存在**，只是生效场景是
      「finalize 循环中途某个文件校验失败」：先提交的已被 `remove_created_sink` 摘掉、
      后面没轮到的还在清单里。`finalized_file_survives_sibling_cleanup` 覆盖的正是这个语义。
      「要不要改成逐文件 finalize」是另一条 change 的议题，本 change 不动它。
- [ ] 7.18 Android 前台服务通知上的「暂停」「取消」按钮，在**发送**与**接收**两个方向各点一次
      （对应 6.6–6.8；这条不在页面里，是最容易漏的回归）

### 手动验收 —— #98

- [ ] 7.19 粘贴一条有效邀请 → **先**出现确认卡，展示设备名 / 平台 / 剩余有效期；
      此时 DevTools Network 面板与 wasm 日志**均无出网**
- [ ] 7.20 点「取消」→ 邀请未被消费：回到邀请方看该条仍是「等待对方使用」，
      且同一条串可以再次粘贴并成功配对
- [ ] 7.21 点「确认」→ 走通原有 `connect_invite`，配对成功
- [ ] 7.22 格式非法 / 被篡改一个字符的邀请 → 确认卡阶段即拒，文案指向「链接不对」
      而非「网络错误」（对应 2.7）
- [ ] 7.23 已过期邀请 → 确认卡阶段即拒，说「已过期」（对应 5.7）
- [ ] 7.24 **已撤销**邀请 → 确认卡照常出现（本地判不出），点确认后由邀请方拒绝，
      前端渲染成人话（对应 5.10 / design D5）
- [ ] 7.25 从 `/p/` 落地页跳转过来的邀请（sessionStorage 主路径与 fragment 兜底**各测一次**）
      同样经过确认卡
- [ ] 7.26 自我过滤：本机生成一条邀请 → **刷新页面**（`generatedInvite` 已为 null）→
      粘贴自己那条 → 不亮确认卡（旧判据在这一步必漏，是 5.4 的验收点）
- [ ] 7.27 已配对过滤：与某设备配对完成后再粘贴它的另一条邀请 → 提示「已经配过了」

### 收尾

- [x] 7.28 `dev-notes/knowledge/web-app-frontend.md` 补一条：`crates/web` 新增导出的三处
      生成链路（bindings.ts / `docs/packages/swarmdrop-web` / `view-types.ts` 手工再导出），
      漏任一处的症状是什么（design D10）
- [x] 7.29 `dev-notes/knowledge/storage-abstraction.md` 或 `web-app-frontend.md` 记一条：
      **`FileAccess::cleanup_sink` 的端口契约没写「要删除部分产物」，默认实现是 no-op**，
      Web 已在本 change 补上，桌面本就在删，**移动端未核实** —— 留给后续端口层 change
- [ ] 7.30 关闭 #98 / #99 时贴上 7.19 / 7.13 两条的证据（零出网截图、OPFS 前后对比）
