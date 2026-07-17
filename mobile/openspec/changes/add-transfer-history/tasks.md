## 1. 共享 core 与依赖准备

- [x] 1.1 ~~确认 `swarmdrop-core::database::ops::mark_session_failed` 已接受 `Option<String>`~~ → 验证完成：实际签名是 `mark_session_failed(db, session_id, error_message: &str)`，直接传 `"app_interrupted"` 字面量即可，**不需要 bump 上游**
- [x] 1.2 确认 `entity::TransferSession` 提供按 status 过滤查询的能力 → 验证完成：`entity::TransferSession::find().filter(entity::transfer_session::Column::Status.eq(SessionStatus::Transferring)).all(db)` 可用
- [x] 1.3 ~~bump Cargo.toml git ref~~ → 不需要：所有 ops 在 develop 当前 ref 已就绪
- [x] 1.4 添加 `expo-sharing@^14.0.0` 到 `package.json`，`pnpm install` 完成（14.0.8 实际安装）；iOS pod / Android autolink 留待原生 build 时自动生效

## 2. mobile-core 类型层（Rust）

- [x] 2.1 拆出新文件 [history.rs](packages/swarmdrop-core/rust/mobile-core/src/history.rs)；`MobileSessionStatus` enum 5 个变种已就位
- [x] 2.2 双向 `From` 实现（`SessionStatus` ↔ `MobileSessionStatus`）
- [x] 2.3 `MobileTransferHistoryFile` Record 已就位
- [x] 2.4 `MobileTransferHistoryItem` Record 已就位（含 updated_at 字段）
- [x] 2.5 `From<ops::TransferHistoryItem>` / `From<ops::TransferHistoryFile>` 直接复用共享 crate 已有 ops 返回类型，避免重复操作 entity
- [x] 2.6 `MobileResumeTransferResult` Record 已就位（files 字段复用 events.rs 既有的 `MobileTransferResumedFile`）
- [x] 2.7 `ERROR_APP_INTERRUPTED` 常量已加到 [error.rs](packages/swarmdrop-core/rust/mobile-core/src/error.rs)

## 3. mobile-core API 层（Rust uniffi exports）

- [x] 3.1 `MobileCore::list_transfer_history` 实现完成
- [x] 3.2 `MobileCore::get_transfer_session_detail` 实现完成（共享 crate 的 `get_session_detail` 已经处理 404）
- [x] 3.3 `MobileCore::delete_transfer_session` 实现完成（不存在时静默跳过，与共享 crate 行为一致）
- [x] 3.4 `MobileCore::clear_transfer_history` 实现完成
- [x] 3.5 `MobileCore::resume_transfer` 实现完成（按 DB 中 direction 字段分发）
- [ ] 3.6 ~~单元测试~~ → 移到 Group 11 真机测试覆盖；mobile-core 整体没有现成的 test harness 与 in-memory SQLite fixture（共享 crate 的 ops 已有 unit test）

## 4. mobile-core 启动 reconcile（Rust）

- [x] 4.1 `reconcile_stale_sessions` 写在 [history.rs](packages/swarmdrop-core/rust/mobile-core/src/history.rs)
- [x] 4.2 [network.rs:65](packages/swarmdrop-core/rust/mobile-core/src/network.rs#L65) `start_node` 中已接入：`ensure_db` 之后、`runtime::start_node` 之前
- [x] 4.3 reconcile 命中时 `tracing::warn!` 已加，总数 summary 用 `tracing::info!`
- [ ] 4.4 ~~单元测试~~ → 同 3.6 理由移到真机回归

## 5. 生成 RN bindings

- [x] 5.1 `cargo build --lib` + `ubrn generate jsi bindings` 走通；生成产物已就位
- [x] 5.2 校验 `MobileCoreLike` 接口的 5 个新方法已存在
- [x] 5.3 校验 `MobileTransferHistoryItem` / `MobileTransferHistoryFile` / `MobileSessionStatus` / `MobileResumeTransferResult` 类型 + FfiConverter 全部生成
- [ ] 5.4 git commit 待用户决定时机（已在 status check 中显示完整 diff）

## 6. RN store 重构

- [x] 6.1 store 引入 native 类型完成
- [x] 6.2 [src/core/transfer-types.ts](src/core/transfer-types.ts) 新建，TransferSession / RegisterSessionInput / TransferOfferQueueItem 定义就位
- [x] 6.3 state shape 重写：sessions / dbHistory / offerQueue / currentOffer / lastError
- [x] 6.4 loadHistory action 实现完成
- [x] 6.5 clearAllHistory / deleteHistoryItem / resumeHistoryItem 实现
- [x] 6.6 addSession / updateProgress / markAccepted / markRejected / resumedSession / removeAndRefresh / pushOffer / dismissOffer 全套 actions 实现
- [x] 6.7 [src/core/event-bus.ts](src/core/event-bus.ts) 已重新接入所有事件（Progress/Accepted/Rejected/Completed/Failed/Paused/Resumed）
- [x] 6.8 ~~独立 setupTransferListeners~~ → 沿用现有 `EventBus.emit` → `routeEventToStores` 的中央 dispatcher，更小改动面
- [x] 6.9 [select-device.tsx](src/app/send/select-device.tsx) / [transfer-offer-host.tsx](src/components/transfer-offer-host.tsx) 改为调用 addSession 提供完整 metadata；[(main)/index.tsx](src/app/(main)/index.tsx) 用 sessions 派生

## 7. RN UI - 共享组件与工具函数

- [x] 7.1 [src/components/transfer/shared.tsx](src/components/transfer/shared.tsx) 完成：DirectionIcon / StatusBadge / StatusLabel / statusKey / formatBytes / calcPercent / formatRelativeTime / LocalizedError / canShareFile / canResume / canResend
- [x] 7.2 [src/components/history-transfer-row.tsx](src/components/history-transfer-row.tsx) 完成
- [x] 7.3 [recent-transfer-row.tsx](src/components/recent-transfer-row.tsx) 维持原样，语义已明确为活跃卡片（输入 MobileTransferProgress，含 speed/eta）

## 8. RN UI - 传输历史列表页

- [x] 8.1 [src/app/transfer/index.tsx](src/app/transfer/index.tsx) 重写：双 section + EmptyState + FilterBar
- [x] 8.2 FilterBar 用横向 ScrollView + 5 个 Pressable Pill 实现
- [x] 8.3 「清空」按钮 + Alert.alert 确认
- [x] 8.4 useFocusEffect 触发 loadHistory（每次 focus 刷新）
- [x] 8.5 所有文案 `<Trans>` / `t\`\`` 接入

## 9. RN UI - 会话详情页

- [x] 9.1 [src/app/transfer/[sessionId].tsx](src/app/transfer/[sessionId].tsx) 重写：sessions / dbHistory / native fallback 三层查找
- [x] 9.2 「概览」section 完成
- [x] 9.3 「文件列表」section + canShareFile 控制可点击性
- [x] 9.4 expo-sharing 集成，含 isAvailableAsync 检查与失败降级
- [x] 9.5 长按 Clipboard.setStringAsync + toast 完成
- [x] 9.6 操作按钮组按状态分支（活跃：暂停/取消；历史：恢复/重新发送/删除）
- [x] 9.7 「重新发送」走 prepareSend + sendPrepared + addSession + router.replace
- [x] 9.8 「删除」Alert 确认 + deleteHistoryItem + router.back
- [x] 9.9 全部文案 i18n

## 10. 国际化文案补全

- [x] 10.1 `pnpm i18n:extract` 完成（234 条 messages）
- [x] 10.2 zh-Hans 作为 source，全部就位
- [x] 10.3 en 全部 234 条翻译完成（0 missing）
- [x] 10.4 ~~`pnpm lingui compile`~~ → 项目使用 `@lingui/metro-transformer` 在打包时 on-the-fly 编译，无需手动 compile

## 11. 手动测试（真机）

- [ ] 11.1 happy path：A 发文件到 B，B 在「传输历史」页看到 active → completed 的过渡，期间不出现空窗
- [ ] 11.2 失败 path：A 发文件到 B，传输中 B 断网 → 历史出现一条 failed，错误原因可读
- [ ] 11.3 暂停 / 恢复 path：A 发大文件到 B，A 点暂停 → 双方历史显示 paused → A 点恢复 → 新 transferring session 建立
- [ ] 11.4 reconcile path：传输中强杀 RN 进程 / iOS 后台被系统回收 → 重启后该会话状态为 failed，error_message 显示为"上次未完成"
- [ ] 11.5 详情页分享：在 iOS / Android 上分别测试 `Sharing.shareAsync` 能拉起系统分享面板，文件可以保存到「文件」App / 分享给微信等
- [ ] 11.6 详情页长按复制：长按文件行后剪贴板内有正确的文件 URI
- [ ] 11.7 重新发送：completed 的发送历史，点重新发送 → 复用文件 → 接收端弹出新 offer
- [ ] 11.8 清空 / 删除：清空后历史空、删除单条后该条消失，重启 app 不复现
- [ ] 11.9 状态过滤：每种过滤选项都能正确收窄结果，空过滤时 section 整体隐藏
- [ ] 11.10 i18n：把系统语言切英文，所有新增文案正确显示

## 12. 收尾

- [x] 12.1 `pnpm typecheck` + `pnpm lint` + `cargo check --lib` 全绿
- [ ] 12.2 iOS 真机 release build 待用户操作（需 `pnpm build:ios:release` + 物理设备）
- [ ] 12.3 Android 真机 release build 待用户操作（需 `pnpm build:android:release` + 物理设备）
- [ ] 12.4 项目当前没有 CHANGELOG.md，建议在 PR description 里写 user-facing changes
- [ ] 12.5 dev-notes/knowledge 留待用户决定要不要补条目（这次改动量已经够大，知识沉淀可独立任务）
- [ ] 12.6 git commit + PR 待用户决定时机
