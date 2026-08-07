# invite-persistence 任务分解

## Phase 0 — 端口与 TTL（crates/invite）

- [x] `INVITE_TTL_SECS`：300 → 86400，文档注释记下改动理由
- [x] `store.rs` 定义 `InviteStore` trait：`load_all` / `upsert` / `remove` / `prune_expired`，
      `#[async_trait]`，只依赖本 crate 类型（保持 wasm-clean）；另附 `NoopInviteStore`
      供不需要持久化的宿主与测试
- [x] 落盘记录 `InviteRecord`：`capability_hash`(sha256) / `inviter_id` / `expires_at` /
      `state` / `created_at` —— **无 capability 明文、无邀请全串**
- [x] `PersistedInviteState`（落盘态，两值）与 `invite.rs` 内部的 `InviteState`（内存态，
      含 `Revoked`）**刻意分名**：撤销在落盘侧就是删行，没有「已撤销」这个持久状态
- [x] `InviteRegistry` 持 `Arc<dyn InviteStore>`；`register` / `try_consume` / `revoke` /
      `prune_expired` 改 async，形态统一为「**锁内改内存 → 释放锁 → await 写穿**」
      （`std::sync::MutexGuard` 不能跨 await，这个形态同时满足 design D2 的顺序要求）
- [x] 写库失败不回滚内存态（实现层记 warn，端口方法不返回错误）
- [x] `load(now)`：启动读回内存表 + 清过期（内存与库一起）
- [x] `list_active(now)` → `InviteSummary`（哈希 + 时间 + consumed，**无 capability**）
- [x] `revoke_by_hash`：列表撤销只有哈希，因为明文不出注册表
- [x] 单测 20 项：并发双花仍恰有一胜（真线程抢锁 + block_on）、跨重启保留已消费状态、
      过期不进内存表且从库清掉、列表倒序与 consumed 显示、撤销后出列表
- [x] `./scripts/check-wasm.sh` 绿（async-trait 在 wasm 下无碍）

## Phase 1 — native 实现

- [x] `crates/entity/src/pair_invite.rs`（表名 `pair_invites`，capability_hash 为主键）
- [x] `crates/migration/src/m20260730_000001_pair_invites.rs` + `expires_at` 索引 +
      回滚测试（照仓库惯例用 `up_through` 而非写死步数）
- [x] `crates/storage-sql/src/invite.rs`：`SqlInviteStore`，`on_conflict` 更新状态；
      单测 6 项含「清理只按过期时间、已消费未过期的留着」
- [x] `crates/core`：`PairingManager::new` / `NetManager::new` / `start_node` 注入端口；
      `start_node` 内 `load_invites()`（**必须在对外服务前**，否则重启后邀请全成「不认识」）
- [x] `src-tauri`：注入 `SqlInviteStore`；`generate_pair_invite` / `revoke_pair_invite` 加 `.await`
- [x] `mobile-core`：同上（复用同一 SQL 实现）
- [x] core 的四个集成测试补 `NoopInviteStore` 参数

## Phase 2 — wasm 实现

- [x] `crates/web/src/idb.rs`：新增 `INVITE_STORE` 常量，`DB_VERSION` 2 → 3
      （**加 store 必须同时提版本号**，否则 `onupgradeneeded` 不触发）
- [x] `crates/web/src/invite_store.rs`：`IdbInviteStore`，`SendWrapper` 裹 `JsFuture`；
      `prune_expired` 只能读回来逐条删（IndexedDB 无条件批删，邀请表个位数量级可忽略）
- [x] `WebNode::spawn` 注入；`generate_invite` / `revoke_invite` 变 async（wasm 侧签名变化）
- [x] Web 前端 `pairing-panel.tsx` 跟上 async；`pnpm build:wasm` 重新生成产物

## Phase 3 — 邀请列表与撤销

- [x] 后端 `list_invites` / `revoke_invite_by_hash`（core）
- [x] 桌面 IPC：`list_pair_invites` / `revoke_pair_invite_by_id` + `PairInviteListItem` DTO
      （hex 字符串跨 IPC，不把 `[u8; 32]` 暴露给前端）+ `collect_commands!` 登记 +
      `cargo test export_ts_bindings` 再生 bindings.ts
- [x] wasm 导出：`WebNode::list_invites` / `revoke_invite_by_id` + `InviteListItemJson`
- [x] mobile uniffi：`list_pair_invites` / `revoke_pair_invite_by_id` + `MobileInviteListItem`
- [x] 桌面 UI：`settings/-sent-invites-section.tsx`（列表 + 剩余有效期 + 已使用标记 + 撤销），
      挂在设置页。**放设置页而不是配对生成屏**：邀请活 24h 且跨重启存活，「我有几条在外面飘」
      是管理问题；配对屏是一次性流程屏，塞管理清单会让那条流程变长
- [x] Web UI：`pairing-panel.tsx` 加列表与撤销
- [x] 倒计时文案：新增 `formatTimeLeft`（按量级切粒度）替换 `formatCountdown` —— 后者是
      `m:ss`，24h TTL 下会渲染成「1439:59」；同时把前端 `INVITE_TTL_SECS` 常量同步到 86400
- [x] `pnpm i18n:extract` + 补齐新增 11 串的 en / zh-TW 译文
- [ ] 移动 UI：同上（**需先 `pnpm --filter react-native-swarmdrop-core build:ios` 重建 uniffi
      绑定**，JS 侧才拿得到 `listPairInvites` / `revokePairInviteById`）
- [ ] 冒烟：生成 → 列表可见 → 撤销 → 对方点链接被拒

## Phase 4 — 门禁

- [x] `cargo fmt --all` / `cargo check --workspace --all-targets` / `cargo test --workspace`
      （47 个测试组全绿）
- [x] `./scripts/check-wasm.sh`
- [x] `pnpm exec tsc --noEmit`、`pnpm test`（64 passed）、`docs` 下 `pnpm build`、
      `pnpm check:clipboard` / `check:zustand-access`、`mobile` 下 `pnpm typecheck`
- [x] **安全审**：落盘路径（storage-sql / web invite_store / entity / migration）只出现
      `capability_hash`；registry 交给 store 的字段只有哈希；三处 tracing 均无邀请全串或
      capability 明文
- [x] 知识库：`storage-abstraction.md` 新增「第三个走同一模式的端口」一节 —— 端口方法
      何时可以不返回错误、「锁内改内存 → 释放锁 → await 写穿」的固定形态、
      落盘失败不回滚的方向选择，以及那条被推翻的 fail-closed 直觉

## 实施中对 artifacts 的修正

1. **design D5 的安全推理是错的，已改正**。原写「已消费条目不能立即删，否则一次性语义在
   重启后失效」。读代码发现注册表是 fail-closed 的（`.ok_or(InviteRejectReason::Unknown)?`
   —— 查不到即拒绝），删早了只会让它变「不认识」，不会放行。保留 `Consumed` 到过期的真实
   理由是 **UX**（让发起方看到「已被使用」而不是凭空消失）。这个区别决定了将来要优化表大小
   时可以牺牲哪一边。
2. **落盘状态类型改名为 `PersistedInviteState`**。`invite.rs` 里已有一个私有的 `InviteState`
   （三态，含 `Revoked`），同名会让「内存态 / 落盘态」两个概念混在一起。
3. **顺带修了一个既有 breakage**：`crates/web/tests/specta_export.rs` 引用了早已不存在的
   `NodeAddrJson`（6 位分享码时代的 `lookup_share_code()` 返回类型），导致 web 的 bindings
   导出测试在 HEAD 就编不过、入库的 `bindings.ts` 一直没人能再生。已清掉那个死类型。

## 第二轮审查（registry async 落盘）的修复

审查用探针实测推翻了我自己的两处论证 —— 都已改：

- [x] **写方法返回 `bool`**（`upsert` / `remove`）。根因是端口吞掉了写穿失败，注册表根本
      不知道状态没落地。用 bool 而非 Result：端口层没有统一错误类型，调用方只需要
      「成没成」这一位，详情归实现层日志
- [x] **`try_consume` 写穿失败即 fail-closed**（新增 `InviteRejectReason::NotPersisted`，
      core 映射成明确错误而不是笼统的「邀请无效」）。能这么做是因为调用点顺序：
      `respond_pairing_request` 里它排在 `responder.send(Success)` **之前**
- [x] **`revoke` 返回是否落盘**，一路传到桌面与 Web 的 UI（撤销没落盘要告诉用户
      「重启后可能恢复」）。生成新邀请时顺手作废旧的那条路径刻意忽略返回值 —— 用户的
      注意力在新邀请上，为一条他不打算再用的旧邀请弹提示只是噪音
- [x] **`load` 状态单调**：内存里已到终态的条目不会被库里的 `Pending` 盖回去
- [x] **补 4 条测试**：写穿窗口内的中间态仍 fail-closed（测试桩支持挂起，原来的
      `TestStore::upsert` 立刻完成、那个窗口在测试里根本不存在）、消费写穿失败必须报错、
      撤销写穿失败必须可见、`load` 不降级。27/27 绿
- [x] **修正两处被推翻的论证**：`PersistedInviteState` 与 entity 的注释原来说「撤销写状态
      而非删行所以 fail-closed」—— 实测两者后果相同（库里都留 `Pending`），`Revoked` 独立
      于 `Consumed` 的真实理由只是 UX（列表要能区分）。design D2 的「复活一次仍可撤销」
      这条缓解也删了：写穿失败时列表显示「等待对方使用」，用户看不到痕迹、不会想去撤销

## 审查带出的待办（2026-07-30）

- [ ] **IndexedDB `DB_VERSION` 3 的回滚不可逆**：`docs.yml` 从 main / develop 双分支自动部署，
      revert 一次提交很正常，但已访问过 v3 的浏览器本地 DB 就是 v3，回滚后的代码用
      `open_with_u32(DB_NAME, 2)` 会拿到 `VersionError`。`idb.rs` 的 `open()` 把它当
      `WebError::storage` 上报（不 panic），所以是「Web 端存储层整体报错」—— 收件箱、
      传输历史、邀请列表一起失效，且**用户侧无法自愈**（要手动清站点数据）。
      两条出路：明确「idb 版本号只进不退，回滚必须连带 hotfix」，或让 `open()` 在
      `VersionError` 时退回不带版本号打开。**这是新发现，不是已知取舍。**
- [ ] **端口没有 per-key 顺序或版本约束**：任何两个重叠写入都可能乱序落地。审查实测
      「revoke 的写穿先落地、consume 的后落地」→ 库里成了 `Consumed`，撤销被覆盖
      （仍 fail-closed，只是观感）。而 `register` 与 `revoke` 乱序会指向放行 —— 那条实际
      不可达，因为 `encode_invite` 先 `await register` 再返回邀请串，撤销不可能早于它。
      这个「实际安全但靠调用点顺序兜着」的性质值得在端口文档里写明。
