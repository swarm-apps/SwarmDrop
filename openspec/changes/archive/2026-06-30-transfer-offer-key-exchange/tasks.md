## 1. 协议层扩展

- [x] 1.1 在 `protocol.rs` 中定义 `FileInfo` 结构体（file_id, name, relative_path, size, checksum）
- [x] 1.2 在 `protocol.rs` 中定义 `TransferRequest` 枚举（Offer 变体 + 预留 ChunkRequest/Complete/Cancel 注释）
- [x] 1.3 在 `protocol.rs` 中定义 `TransferResponse` 枚举（OfferResult 变体 + 预留 Chunk/Ack 注释）
- [x] 1.4 扩展 `AppRequest` 添加 `Transfer(TransferRequest)` 变体
- [x] 1.5 扩展 `AppResponse` 添加 `Transfer(TransferResponse)` 变体
- [x] 1.6 确认所有新类型实现 Serialize/Deserialize，使用 `serde(rename_all = "camelCase")`

## 2. 后端 OfferManager

- [x] 2.1 创建 `src-tauri/src/transfer/offer.rs`，定义 `PreparedTransfer` 结构体和 `PendingOffer` 结构体
- [x] 2.2 实现 `OfferManager`：`DashMap<String, PreparedTransfer>`（发送方缓存）+ `DashMap<u64, PendingOffer>`（接收方缓存）
- [x] 2.3 实现 `OfferManager::prepare()` 方法：接收文件路径列表，spawn_blocking 中扫描文件、计算 SHA256、分配 fileId，返回 PreparedTransfer
- [x] 2.4 实现 `OfferManager::cache_inbound_offer()` 方法：缓存入站 Offer 的 pending_id、peer_id、session_id、files
- [x] 2.5 实现 `OfferManager::accept()` 方法：从缓存取出 PendingOffer，生成密钥，返回 (pending_id, key)
- [x] 2.6 实现 `OfferManager::reject()` 方法：从缓存取出 PendingOffer，返回 pending_id
- [x] 2.7 在 `transfer/mod.rs` 中导出 `offer` 模块

## 3. NetManager 集成

- [x] 3.1 在 `NetManager` 中添加 `offer: Arc<OfferManager>` 字段，构造函数中初始化
- [x] 3.2 添加 `NetManager::offer()` 访问器方法
- [x] 3.3 在 `SharedNetRefs` 中添加 `offer: Arc<OfferManager>` 字段
- [x] 3.4 更新 `NetManager::shared_refs()` 传递 offer 引用

## 4. 事件循环处理 Transfer 请求

- [x] 4.1 在 `event_loop.rs` 中移除 `#[expect(irrefutable_let_patterns)]`，改为 `match request` 分支
- [x] 4.2 添加 `AppRequest::Transfer(TransferRequest::Offer { .. })` 匹配分支
- [x] 4.3 实现入站 Offer 处理：检查是否已配对 → 未配对自动拒绝 → 已配对则缓存并 emit `transfer-offer` 事件
- [x] 4.4 定义 `TransferOfferPayload` 结构体（sessionId, peerId, deviceName, files, totalSize）
- [x] 4.5 未聚焦时发送系统通知 "收到文件传输请求"

## 5. Tauri 命令实现

- [x] 5.1 实现 `prepare_send` 命令：从 Tauri state 获取 OfferManager，调用 prepare()，返回序列化结果
- [x] 5.2 实现 `start_send` 命令：从缓存取出 PreparedTransfer，筛选 fileIds，构造 Offer，调用 client.send_request()，处理 OfferResult 响应
- [x] 5.3 实现 `accept_receive` 命令：调用 OfferManager::accept()，用 pending_id 和 key 构造 OfferResult，调用 client.send_response()
- [x] 5.4 实现 `reject_receive` 命令：调用 OfferManager::reject()，构造拒绝的 OfferResult，调用 client.send_response()
- [x] 5.5 在 `commands/mod.rs` 中导出四个新命令
- [x] 5.6 在 `lib.rs` 的 `invoke_handler!` 中注册 `prepare_send`、`start_send`、`accept_receive`、`reject_receive`

## 6. 前端接收页改造

- [x] 6.1 在 `src/routes/_app/send/-file-tree.ts` 中实现 `buildTreeDataFromOffer(files: TransferFileInfo[]): TreeData`
- [x] 6.2 在接收页 `receive/index.lazy.tsx` 中导入 `buildTreeDataFromOffer` 和 `FileTree` 组件
- [x] 6.3 替换 `OfferFileList` 为 `<FileTree mode="select">`，传入构建好的 dataLoader 和 rootChildren
- [x] 6.4 移除接收页中不再使用的 `OfferFileList` 组件
- [x] 6.5 确保 SavePathPicker 功能保持不变

## 7. 前端命令对接

- [x] 7.1 更新 `src/commands/transfer.ts`：移除 stub 注释，确认类型定义与后端一致
- [x] 7.2 确认 `transfer-store.ts` 的事件监听能正确处理后端 emit 的 `transfer-offer` 事件

## 8. 验证

- [x] 8.1 `cargo build` 编译通过
- [x] 8.2 `cargo test` 所有测试通过
- [x] 8.3 `pnpm build` 前端编译通过
