# swarmdrop-web —— 浏览器传输端

让浏览器成为真正的 SwarmDrop 传输端：**offer / accept / 续传 / bao 逐块验证全量复用**
`swarmdrop-transfer` 内核，端口（`SessionStore` / `FileAccess` / `TransferEventSink`）用 Web
实现填充（内存表 / OPFS / ReadableStream）。范围内**无配对持久化**（正式配对 / React UI 属
后续前端工程）。

## crate 形态

- `#![cfg(wasm_browser)]` 门控整 crate：native target 下是**空壳**（`cargo check --workspace`
  秒过、进 CI），只有 `wasm32-unknown-unknown` 下是真身。
- 进 `scripts/check-wasm.sh` 的五 crate 双 target 门禁。

## 构建

```sh
# 依赖：wasm-pack；macOS 还需 Homebrew LLVM（Apple clang 无 wasm backend，ring 等 C 依赖编 wasm 必挂）
brew install llvm   # macOS only

cd crates/web

# macOS：指向 Homebrew LLVM（Linux 通常可省）。getrandom backend 由仓库根 .cargo/config.toml 提供。
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar

wasm-pack build --target web --release --out-dir static/pkg

# 起静态服务器
python3 -m http.server 8080 -d static
```

> ⚠ **必须用 secure context 访问（https 或 localhost / 127.0.0.1），否则接收方落盘失败。**
>
> 非 secure 源（**http 私网 IP** 如 `http://192.168.x.x:8080`）下浏览器**不提供 `navigator.storage`
> 与 `crypto.subtle`**——接收方 finalize 走 OPFS 落盘时 `getDirectory()` 的 promise 永久 pending
> （现已 5s 超时 + 启动横幅预警，不再静默挂死）。secure context 仅含 `https://` /
> `http://localhost` / `http://127.0.0.1`。**端到端收发实测请用 `http://127.0.0.1:8080`**。
>
> 张力：测「私网 ws 的 mixed-content 豁免」那格需要私网 IP，但那样 OPFS 落盘用不了——两者
> 不可兼得，按测试目标选 origin。（记录见 `dev-notes/knowledge/libp2p-wasm.md`。）

## 使用（页面）

1. **connect**：填一个 helper 的 `ws` / `webrtc-direct` 地址（带 `/p2p/<id>` 尾段）→ connect；
   再 **reserve** 拿到 circuit 地址（浏览器被动接收连接的唯一入口，供对端拨回）。
2. **分享码**（可选）：需已连一个 DHT-capable helper（浏览器不可达 TCP bootstrap）；lookup 后
   自动回填对端 node id + 地址。
3. **发送**：填对端 node id、选文件、send —— 走内核 prepare（blake3 checksum + bao outboard）
   → Offer → 对端接受后推送（每块带 bao proof）。
4. **接收**：对端 Offer 到达 → 「收到的 Offer」区出现条目 → 接受/拒绝。接受后逐块验证落 OPFS，
   完成后出现下载链接（读回 OPFS 建 blob URL）。

## API 面（`WebNode`，wasm-bindgen）

| 方法 | 说明 |
|---|---|
| `spawn()` | localStorage 身份 → Browser preset + DHT client → 装配 TransferManager + Router |
| `node_id()` | 本机 base58 身份 |
| `connect(addr)` / `reserve(helper_addr)` | 拨地址 / 请求 circuit reservation |
| `lookup_share_code(code)` | DHT 查分享码 → 对端 `NodeAddr` JSON（复用 core record 结构，只读） |
| `send_files(to, files)` | 登记文件源 → prepare → Offer；返回 session_id |
| `pending_offers()` | 当前挂起入站 offer（JSON 数组） |
| `accept_offer(sid)` / `reject_offer(sid)` | 接受（落 OPFS）/ 拒绝 |
| `resume(sid)` | 手动发起断点续传 |
| `download_url(relative_path)` | 完成后读回 OPFS 建 blob URL 供下载 |
| `events()` | 传输事件 `ReadableStream`（`TransferEvent` 序列化对象；**只能取一次**） |
| `close()` | 关停 |

## 端口实现取舍

- **MemorySessionStore**（内存 `SessionStore` + `InboxStore`）：entity `Model` 是纯 scalar 结构，
  直接手构造；投影直接构造 `TransferProjection`（绕开 `ModelEx` 的 `HasMany`），故本 crate
  **不直接依赖 sea-orm**。InboxStore no-op。
- **OpfsFileAccess**：主线程 async OPFS（`navigator.storage.getDirectory / createWritable`；
  **禁用 SyncAccessHandle**——Worker-only，与 webrtc-websys 主线程约束冲突）。JsValue `!Send`
  用 `send_wrapper::SendWrapper` 裹 JsFuture 满足端口 Send。接收侧块入内存缓冲、finalize 一次落
  OPFS（大文件吃内存，流式 positioned write 留作后续）。
- **WebEventSink**：`TransferEvent` 走无界 channel（`Send`）→ `events()` 的 ReadableStream 单点
  消费、serde-wasm-bindgen 序列化（镜像 `WebTransferEvent`，`tag="type"` camelCase）。
- **身份**：`SecretKey` protobuf 编码 hex 存 localStorage。

## 遗留 / 取舍

- **无配对**：`PeerDirectory` 对任意对端返回「陌生、需手动确认」的**合成** `PairedDeviceInfo`
  （Collaborator，auto_accept=false → policy RequireConfirmation）。`incoming.rs` 对未配对
  （`None`）offer 硬拒 `NotPaired`（桌面安全边界），故 Web 无配对时必须给个 `Some`——语义正是
  「陌生设备手动确认」，**不改 transfer**。
- **IndexedDB 持久化未做**（加分项）：内存版足够验证端到端；跨刷新续传属后续（`SendWrapper` 包
  JsFuture 的 Send 方案已在 storage-abstraction.md 探针证可行）。
- **接收缓冲整文件入内存**：demo 取舍，大文件会吃内存。
- DHT 查分享码需先连 DHT-capable helper（浏览器不可达 TCP bootstrap，故 spawn 不加 bootstrap）。
