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

产物是一个 npm 包（`swarmdrop-web`），构建到 **`docs/packages/swarmdrop-web/`**（提交入库），
docs 经 pnpm workspace 以 `workspace:*` 引用、在 `/try` 测试页 import 它。测试 UI 已迁到
docs（`docs/app/try/page.tsx`），本 crate 不再自带 HTML/JS harness。

```sh
# 依赖：wasm-pack；macOS 还需 Homebrew LLVM（Apple clang 无 wasm backend，ring 等 C 依赖编 wasm 必挂）
brew install llvm   # macOS only

# macOS：指向 Homebrew LLVM（Linux 通常可省）。
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar

# ⚠ RUSTFLAGS 必带：wasm-pack 的 cargo 调用**不吃**仓库根 .cargo/config.toml 的 rustflags，
#   getrandom 的 backend cfg 必须在此显式给（feature 侧由本 crate Cargo.toml 的
#   getrandom(0.3/0.4) wasm_js 提供，cfg + feature 缺一不可）。
export RUSTFLAGS='--cfg getrandom_backend="wasm_js"'

# 从仓库根跑（docs 是构建产物家；也可用 docs 的 `pnpm build:wasm` 便捷脚本）
wasm-pack build crates/web --target web --release --out-dir ../../docs/packages/swarmdrop-web

# 跑测试页（docs 独立 workspace）
cd docs && pnpm install && pnpm dev   # http://localhost:3000/try
```

> `--out-dir` 相对**crate 目录**（`crates/web`）解析，故 `../../docs/...`。改 Rust 后重跑本命令
> 重生成 pkg 并提交。

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
| `spawn()` | 持久化身份（Window=localStorage / Worker=OPFS）→ Browser preset + DHT client → 装配 TransferManager + Router |
| `node_id()` | 本机 base58 身份 |
| `connect(addr)` | 拨地址 → `ConnectionJson`（`{ path: "local"\|"direct"\|"relayed", addr }`） |
| `reserve(helper_addr)` | 请求 circuit reservation → circuit 地址字符串 |
| `lookup_share_code(code)` | DHT 查分享码 → `NodeAddrJson`（`{ id, addrs }`） |
| `send_files(to, files)` | 登记文件源 → prepare → Offer；返回 session_id |
| `pending_offers()` | 当前挂起入站 offer → `OfferJson[]` |
| `accept_offer(sid)` / `reject_offer(sid)` | 接受（落 OPFS）/ 拒绝 |
| `resume(sid)` | 手动发起断点续传 |
| `download_url(relative_path)` | 完成后读回 OPFS 建 blob URL 供下载 |
| `events()` | `ReadableStream<WebTransferEvent>`（**只能取一次**） |
| `close()` | 关停 |

**TS 类型端到端**：`src/types.rs` 的 JS 可见类型（`WebTransferEvent` / `OfferJson` /
`ConnectionJson` / `NodeAddrJson` / `WebError`）由 specta 导出成 `static/types/bindings.ts`
（`cargo test -p swarmdrop-web --features specta` 生成，入库），node.rs 经
`typescript_custom_section` 注入 .d.ts 并用 `typescript_type` 把方法签名接到具名类型——
`.d.ts` 里无 `any`。错误 reject 的是 `WebError`（`{ kind, message }`），Worker 桥原样透传
（client.js 把 kind 挂回 Error 实例）。

## 端口实现取舍

- **MemorySessionStore**（内存 `SessionStore` + `InboxStore`）：entity `Model` 是纯 scalar 结构，
  直接手构造；投影直接构造 `TransferProjection`（绕开 `ModelEx` 的 `HasMany`），故本 crate
  **不直接依赖 sea-orm**。InboxStore no-op。
- **OpfsFileAccess**：主线程 async OPFS（`navigator.storage.getDirectory / createWritable`；
  **禁用 SyncAccessHandle**——Worker-only，与 webrtc-websys 主线程约束冲突）。JsValue `!Send`
  用 `send_wrapper::SendWrapper` 裹 JsFuture 满足端口 Send。接收侧**流式落盘**：`create_sink`
  开 `createWritable` 句柄常驻，每 chunk `WriteParams{position,data}` positioned 直写（单次
  Promise 往返），`finalize` 时 `close` 提交；续传走 `keepExistingData:true`。大文件不进内存。
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
- DHT 查分享码需先连 DHT-capable helper（浏览器不可达 TCP bootstrap，故 spawn 不加 bootstrap）。
- **client.js 手工镜像 WebNode 方法表**（结构性负债）：新增 WebNode 方法必须同步 client.js
  转发一行，否则 Worker 模式静默缺方法（曾漏过 pending_offers/close）。根治可用 Proxy 动态
  转发，React UI 工程时一并处理。
- **2026-07-19 全 crate 审查记录为后续的项**：identity 未走 `KeychainProvider` 端口（trait 含
  migration/配对持久化共 7 方法，Web 暂只需身份 3 个——配对持久化工程时做完整
  `WebKeychainProvider`）；方法名 snake_case 与桌面 bindings.ts 的 camelCase 不一致（`js_name`
  可改，随 React UI 一并）；`content_root_of` 与 transfer 版重复（泛化 transfer 签名可归一，
  涉及三 crate 调用点）。
- **`connect()` / `reserve()` 对不可达地址无内建超时**（2026-07-21 `docs/app/app` 连接面板
  `#76` 实测发现）：`connect()` 对无法握手的地址会在数十秒后 reject（swarm 拨号重试耗尽），但
  `reserve()` 对同类地址可**无限期挂起** JS Promise（swarm 持续退避重试拨号，reserve 等的
  circuit 事件永远不来）。JS 侧目前**没有**任何客户端可见的超时或取消口子。前端调用方必须自
  行套一层超时兜底（`docs/app/app/_components/connection-panel.tsx` 的 `withTimeout`，20s），
  否则 UI 会卡在「reserve 中…」不给反馈，违反「状态诚实可见」。根治需要内核加超时/可取消的
  reserve，或 wasm 侧暴露 abort 口子；当前判定为前端职责，未改内核。

## 基准（`static/bench.html` + `scripts/web-bench/driver.mjs`）

自动化传输基准：headless Chrome 双 tab（同 origin，send 侧 spawn 前清 localStorage 身份），经
本机 helper 走 relay circuit 互传，测速率 / 主线程卡顿（longtask）/ SHA-256 字节一致性。

```sh
# 前置：wasm-pack build 完成；net-web-smoke helper 运行中；http.server 8080 -d static
node scripts/web-bench/driver.mjs "<helper-ws-addr>/p2p/<id>" 268435456 1
```

2026-07-18 实测（M2 MacBook 同机三跳 A→relay→B，headless，最不利 CPU 竞争配置）；
`recvMode` 第 4 参数选接收侧运行模式（`main` | `worker`）：

| 大小 | 接收模式 | 落盘 | 接收耗时 | 均速 | 接收侧 longtask | hash |
|---|---|---|---|---|---|---|
| 256 MB | 主线程 | createWritable | 8.2s | 31.3 MB/s | **0** | ✅ 一致 |
| 1 GB | 主线程 | createWritable | 32.1s | 31.9 MB/s | **0** | ✅ 一致 |
| 256 MB | Worker | createWritable | 7.4s | 34.8 MB/s | **0** | ✅ 一致 |
| 1 GB | Worker | createWritable | 33.2s | 30.9 MB/s | **0** | ✅ 一致 |
| 256 MB | Worker | **SyncAccessHandle** | 8.2s | 31.1 MB/s | **0** | ✅ 一致 |
| 1 GB | Worker | **SyncAccessHandle** | 33.6s | 30.5 MB/s | **0** | ✅ 一致 |

六组全部 ~31±3 MB/s（run 间噪声级差异）——**瓶颈在网络链路（relay 三跳 + noise），
不在落盘**：SyncAccessHandle 同步直写对吞吐无增益，但每 chunk 省一次 wasm↔JS Promise
调度、写即落盘（无 staging，崩溃丢失面更小），Worker 版保留它。速率不随文件大小衰减
（无内存压力）；接收全程主线程零长任务——**31 MB/s 档收流 + 流式落盘不卡 UI**。
发送侧仅准备段一次 longtask（`new File([buf])` 构造测试数据，非传输热路径）。

## Worker 运行模式（`static/worker.js` + `static/client.js`）

同一份 wasm 双环境通吃：`WebNode.spawn()` 在 Window 直跑（webrtc+ws 双 transport），
在 Web Worker 里跑则自动 ws-only（`env.rs` 探测；**webrtc-websys 在 Worker 装着都不行**——
它的 dial 在地址格式检查之前就碰 window，经 or_transport 拨任何地址都 panic，故 transport
组装按环境裁剪）。身份持久化双轨：Window=localStorage、Worker=OPFS 文件（Worker 全自治，
无需主线程注入）。落盘句柄也按环境二选一（`SinkHandle`）：Window 用 `createWritable`
staging 流，Worker 用 **`SyncAccessHandle`**（同步零 Promise 直写、独占锁须显式 close、
写即落盘无 staging）。`client.js` 提供与 `WebNode` 同形状的主线程桥（postMessage RPC，
事件用 `onEvent` 回调），调用方两版无感切换；`File` 对象经 structured clone 传入 Worker。
