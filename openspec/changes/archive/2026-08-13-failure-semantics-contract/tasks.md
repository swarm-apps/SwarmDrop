# failure-semantics-contract 任务分解

> 五组任务彼此独立，唯一的顺序约束是 **2 在 3 之前**（三端文案表要照着新 kind 写）。
> 每组做完各自跑门禁，最后统一跑一遍全量。

## 1. 越线规则落到传输接受/拒绝路径

- [x] 1.1 `crates/transfer/src/flow/receive.rs`：抽 `prepare_accept(&self, offer, save_location)`
      —— 承载越线前的两步可失败动作（写保存位置、`dispatch(Accept)`）
- [x] 1.2 `accept_and_start_receive` 改为：`remove` → `prepare_accept` → **失败则把 offer
      放回 `pending` 再返回 Err** → `start_receive_actor` → `responder.send`（越线）
- [x] ~~1.3 越线之后的收尾不再 `?`：返回 `AcceptOutcome { degraded }`~~
      **不做，见 design D2**：1.1 把 `dispatch(Accept)` 挪到越线之前后，越线之后一个可失败的
      步骤都不剩，那个标记恒为「正常」。三端接完才发现，已回滚
- [x] 1.4 `responder.send` 失败（越线未发生）走回滚：移除 actor + `dispatch(Cancel)` + 返回 Err
- [x] 1.5 `reject_and_respond` 同规则：`dispatch(Reject)` 挪到 `responder.send` **之前**，
      失败则 offer 放回（不是「改 warn」——那是次优解，见 design D1 的优先级）
- [x] 1.6 测试 `accept_before_the_line_keeps_the_offer_retryable`：删掉会话行制造真实失败，
      断言 offer 仍在待决表、应答通道既没送值也没被 drop
- [x] 1.7 测试 `accept_rolls_back_when_the_peer_already_hung_up`：drop 应答接收端，
      断言 actor 已撤、会话落终态
- [x] 1.8 测试 `reject_before_the_line_keeps_the_offer_retryable`：关连接池制造 dispatch 失败
      （删行不行 —— `dispatch` 查不到 session 返回 `Ok(None)`，会静默成功）
- [x] 1.9 反向验证：把实现改回旧形态，确认 1.6 / 1.7 都会红

## 2. `AppError` 补有语义的 kind

- [x] 2.1 `crates/host/src/error.rs`：新增 `SessionNotFound` / `StorageFailed`，接进手写 `Serialize`
- [x] ~~`IntegrityFailed`~~ **不做，见 design D3**：内容校验失败只在 ReceiverActor 内发生，
      走 `ActorReport::FatalError(String)` 通道，永远到不了 `kind`
- [x] 2.2 给 `Transfer` 写判据 doc（「其余传输失败」+ 两问：能给出不同建议吗 / 到得了 UI 吗）
- [x] 2.3 迁移「不存在」类调用点 → `SessionNotFound`（12 处，限**命令能直接收到**的那些）
- [x] 2.4 迁移落盘失败类 → `StorageFailed`（web opfs 的 JS 错误漏斗 + 三处超时 + 桌面写分块）
- [x] 2.5 `crates/web/src/error.rs` 的穷尽 match 补两臂；`mobile-core` 的 `FfiError` 双向映射同步
- [x] 2.6 重新生成 uniffi 绑定（`ubrn generate jsi bindings --library`，用 host cdylib，
      比 `build:ios --and-generate` 快一个数量级）

## 3. 三端文案表（依赖 2）

- [x] 3.1 桌面 `src/lib/errors.ts`：补 2 条
- [x] 3.2 移动端**新建** `mobile/src/lib/errors.ts`：`isFfiError` / `isErrorKind` /
      `errorDetail`（只给 console）/ `getErrorMessage`
- [x] 3.3 **范围比预想大**：真正的问题不是 5 处裸 `err.message`，是 `lib/utils.ts` 的
      `errorMessage()` —— 它专门把 Rust `inner` 展开成文案，被 **20 处 `toast.error` 当用户文案用**
      （英文界面上弹 `FfiError.Transfer: 收件箱条目不存在`）。已整体替换为 `getErrorMessage`
      并从 `utils.ts` 删除，另 6 处页面 state 一并接上
- [x] 3.4 顺手修 `core/event-bus.ts`：`msg.includes("NodeNotStarted")` → `isErrorKind(err, ...)`
      （对 message 做子串匹配，Rust 改一个字这条静默就失效）
- [x] 3.5 Web `WEB_ERROR_KIND_LABEL` 的 `notFound` / `storage` 改写成动作（原本是名词，说完等于没说）
- [x] 3.6 三端 `i18n:extract` + 补齐 en / zh-TW / zh-Hans 译文（复用桌面已有措辞保持一致）

## 4. Web 保留结构化的配对拒绝原因

- [x] 4.1 `crates/web/src/types.rs`：`PairingOutcomeJson` 加 `refused: Option<PairingRefusedJson>`
      —— 本地投影而非直接用内核类型（`swarmdrop-core` 在本 crate 是 wasm-only 依赖，
      而 types.rs native 也要编）；安全性由 node.rs 的穷尽 match 保证
- [x] 4.2 `connect_invite`：拒绝不再 `Err(WebError::network(中文串))`，改为 `Ok` + `refused`
- [x] 4.3 `pairing-panel.tsx`：`consumeSuccess` → `consumeOutcome`，按 reason 出文案；
      「邀请可能已被撤销/用掉」那句解释从 `consumeAction.error` 移到 refused 分支
      （那才是它真正会命中的地方）
- [x] 4.4 `_lib/view-types.ts` 加 `PAIRING_REFUSED_LABEL`
- [x] 4.5 `pnpm build:wasm` 重新生成 wasm 产物与 .d.ts

## 5. hex 收进 `crates/invite`

- [x] 5.1 `crates/invite/src/store.rs`：`capability_hash_to_hex` / `capability_hash_from_hex`
      + 3 条测试（含多字节输入不 panic 的回归锚点）
- [x] 5.2 删掉 5 份拷贝（web ×2 / storage-sql / src-tauri / mobile-core），
      调用点改用新函数；各自保留自己的日志/错误语义（读库坏行 vs IPC 入参非法）

## 6. 门禁与收尾

- [x] 6.1 `cargo fmt --all` + `check --workspace --all-targets` + `test --workspace` + `clippy`
- [x] 6.2 `./scripts/check-wasm.sh` + `--clippy` + `./scripts/test-wasm.sh`
- [x] 6.3 桌面 `tsc --noEmit` + `pnpm test` + `check:zustand-access` + `check:clipboard`
- [x] 6.4 docs `tsc` + 移动 `typecheck` + biome + mobile cargo fmt/clippy
- [x] 6.5 知识库：越线规则与 kind 判据进 `dev-notes/knowledge/rust-backend.md`；
      移动端文案表进 `mobile/dev-notes/knowledge/`

## 不在本 change 内（已记录，不是遗漏）

- **`TransferFailedEvent.error: String` / `session.error_message` 通道**。会话级失败原因
  （bao 验签失败、源文件变更、对端中止）走的是这条链路，不是命令返回值。移动端已用
  `friendlyTransferError` 的正则匹配对付它，桌面与 Web 没有。收成判别码要动 wire、DB 列、
  恢复逻辑与三端详情页 —— 单独立项。
- **Android 原生库**。`jniLibs/` 是 gitignore 的构建产物；跨平台绑定（`src/generated/*.ts`、
  `cpp/generated/*`）已在 2.6 一次生成并入库，Android 只需本地/CI 重编 `.so`。
