# rust-string-boundary 任务分解

> 四组彼此独立。1 → 2 有顺序（三端文案表照着 `FailureCode` 写）；3、4 可并行。

## 1. `FailureCode` 判别码取代自由文本

- [x] 1.1 `crates/transfer/src/failure.rs`：`FailureCode` enum（`FileFinalizeFailed` /
      `SessionExpired` / `Legacy`），serde tagged + specta 门控；`to_column` / `from_column`
      —— 存量行解析失败归 `Legacy`，**不 panic 不丢数据**
- [x] 1.2 `TransferState.error_message: Option<String>` → `failure: Option<FailureCode>`；
      `ActorReport::FatalError(String)` → `FatalError(FailureCode)`
- [x] 1.3 **构造点是三个不是一个**（调研里写的「唯一」是错的，grep 只扫了 `actor/`）：
      `receiver.rs`（→ `FileFinalizeFailed`）、`flow/resume/mod.rs`（→ `ResumeRejected`）、
      `flow/send.rs::mark_offer_fatal`（→ `OfferFailed`）。`file_id` / 底层错误进 `warn!`
      不进用户串
- [x] 1.4 `expired_receive_reason()` → `FailureCode::SessionExpired { retention_days }`
- [x] **额外收获**：`resume_reject_message()` 把六变体的 `ResumeRejectReason` 摊平成六句
      中文再落库。判别码直接内嵌那个 enum，不再压成字符串（该函数在
      `AppError::Transfer(...)` 那三处保留——那条通道的 message 已经不进 UI）
- [x] 1.5 `TransferProjection.error_message` → `failure`；storage-sql 两个写入点 + 读取点
      改走 `to_column` / `from_column`
- [x] 1.6 `crates/web/src/store.rs` 同步（IndexedDB 侧同一套编解码）
- [x] 1.7 测试：`Legacy` 兜底、round-trip、`FileFinalizeFailed` 不含 file_id

## 2. 三端按 code 出文案（依赖 1）

- [x] 2.1 桌面 `src/lib/errors.ts` 加 `FAILURE_CODE_MESSAGES` + `session-panel` / `-session-row` 接上
- [x] 2.2 Web `_lib/view-types.ts` 加 `FAILURE_CODE_LABEL` + `send-panel` / `transfer-detail` 接上
- [x] 2.3 移动端 **删 `friendlyTransferError` 的 9 条正则**，`LocalizedError` 改吃 code
- [x] 2.4 `mobile-core/src/history.rs` 加 `MobileFailureCode` uniffi 镜像 + drift guard 解构
- [x] 2.5 重新生成 uniffi 绑定
- [x] 2.6 三端 `i18n:extract` + 补齐 en / zh-TW / zh-Hans

## 3. 收件箱标题去中文化

- [x] 3.1 `inbox_title` → 返回首个文件名（0 文件 → 空串），改名 `inbox_primary_file_name`
- [x] 3.2 两个存储实现的写入点跟随；搜索索引 title 列存同一个值
- [x] 3.3 三端渲染按 `itemCount` 0 / 1 / N 三分支
- [x] 3.4 **回填迁移**：`inbox_items.title` / `inbox_search_index.title` 重算为首文件名
- [x] 3.5 Web 端直接换（无真实用户，按 CLAUDE.md 不写迁移）

## 4. `crates/webrtc-p2p` 公开面英文化

- [x] 4.1 19 处 `#[error(...)]` 改英文
- [x] 4.2 公开 API（`pub` 项）的 `///` doc 注释改英文
- [x] 4.3 内部注释 / `tracing` 日志 / 测试断言**不动**

## 5. 门禁

- [x] 5.1 `cargo fmt` + `check --workspace --all-targets` + `test --workspace` + `clippy`
- [x] 5.2 `check-wasm.sh` + `--clippy`
- [x] 5.3 桌面 `tsc` + `pnpm test` + `check:zustand-access` + docs `tsc`
- [x] 5.4 移动 `typecheck` + biome + cargo fmt/clippy
- [x] 5.5 知识库更新
