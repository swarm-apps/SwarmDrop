## Why

SwarmDrop 的文件传输功能目前处于「前端 UI 完整，后端核心逻辑缺失」的状态。前端已有发送页、接收页、进度页和 transfer-store，加密基础设施（TransferCrypto）也已实现并通过测试，但协议层（AppRequest/AppResponse）尚未扩展 Transfer 变体，发送方的 `prepare_send`/`start_send` 命令和接收方的 `accept_receive`/`reject_receive` 命令均未实现。需要打通从发送方准备文件到接收方确认并交换密钥的完整 Offer 流程，为后续分块传输奠定基础。

## What Changes

- 扩展 `protocol.rs` 中的 `AppRequest`/`AppResponse`，添加 `Transfer` 变体（Offer、OfferResult）
- 实现后端 `prepare_send` 命令：扫描文件、计算 SHA256 校验和、分配 fileId，返回 `PreparedTransfer`
- 实现后端 `start_send` 命令：通过 libp2p request-response 发送 Offer 到接收方，等待 OfferResult
- 在事件循环中处理入站 `AppRequest::Transfer(Offer)`：缓存 pending 请求，emit `transfer-offer` 事件到前端
- 实现后端 `accept_receive` 命令：生成 256-bit 对称密钥，通过 `send_response` 回复 OfferResult（含密钥）
- 实现后端 `reject_receive` 命令：回复拒绝的 OfferResult（含 reason）
- 前端接收页改造：新增 `buildTreeDataFromOffer()` 函数，将 Offer 的 flat file list 转为 TreeDataLoader，替换现有 `OfferFileList` 为复用的 `FileTree` 组件
- 在 `NetManager` 中集成 Transfer 状态管理（pending offers 缓存）
- 在 `lib.rs` 中注册新增的 Tauri 命令

## Capabilities

### New Capabilities
- `transfer-offer`: 发送方准备文件并发送 Offer 到接收方，接收方查看文件树后确认/拒绝，确认时交换加密密钥
- `transfer-protocol`: Transfer 协议类型定义（TransferRequest、TransferResponse 及其变体）

### Modified Capabilities
<!-- 无现有 specs 需要修改 -->

## Impact

- **Rust 后端**: `src-tauri/src/protocol.rs`（协议扩展）、`src-tauri/src/commands/transfer.rs`（新增 4 个命令）、`src-tauri/src/network/event_loop.rs`（处理 Transfer 入站请求）、`src-tauri/src/network/manager.rs`（集成 Transfer 状态）、`src-tauri/src/transfer/mod.rs`（新增 session/offer 管理模块）、`src-tauri/src/lib.rs`（注册命令）
- **前端**: `src/routes/_app/send/-file-tree.ts`（新增 buildTreeDataFromOffer）、`src/routes/_app/receive/index.lazy.tsx`（改用 FileTree 组件）、`src/commands/transfer.ts`（移除 stub 标记，对接真实命令）
- **依赖**: 无新增依赖，所有所需 crate（chacha20poly1305、blake3、sha2、walkdir、dashmap）已在 Cargo.toml 中声明
