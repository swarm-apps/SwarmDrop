# SwarmDrop 网络内核重构系列（2026-07）

> 一次把基于 libp2p 的网络内核重写成 iroh 风格 API、扩展到浏览器（wasm）、并把传输域
> 抽成可依赖倒置的独立 crate 的完整记录。拆成 6 个主题系列，每篇聚焦一个内聚知识点。

## 背景

起点：libp2p 0.56 的 `Swarm` + 11 个 behaviour + 命令通道事件循环（旧 `swarm-p2p-core`）。
终点：六层架构 + wire v2 + bao 逐块验证 + 浏览器传输端，浏览器跑与桌面**字面同一份**传输逻辑。

对应 5 个 commit：`refactor(net)!` → `refactor(transfer)` → `feat(transfer): bao` → `feat(web)`。

## 六个系列

### 1. [network-kernel/](network-kernel/) — 学 iroh 重构网络内核
底层保留 libp2p，学 iroh 的**架构边界与 API 表达**。为什么不迁 iroh 却要学它；Endpoint
门面、按协议路由、事件双轨、可插拔扩展点、裸流 typed RPC、类型边界。**架构演进视角。**

### 2. [rust-wasm/](rust-wasm/) — Rust 编译到浏览器实战
「单核心包」：同一份 Rust 逻辑跑桌面与浏览器。双 target 工程、n0-future 垫片、
吃 libp2p git master 的坑、wasm 工具链、以及最重要的一课——**编过 ≠ 能用**。**wasm 工程视角。**
另有两篇存储 / 落盘补线：sea-orm 从 core 摘到 `storage-sql`（core 编 wasm 的存储前置）、WebRTC 流式 + OPFS 落盘不炸内存。

### 3. [transfer-architecture/](transfer-architecture/) — 传输域的架构抽象
dumbpipe 形状、传输域抽独立 crate、依赖倒置的端口 trait、打破事件循环依赖、
bao-tree 逐块验证、删掉应用层加密（加密应该在哪一层）。**软件设计视角。**

### 4. [browser-platform/](browser-platform/) — 浏览器平台知识
OPFS、secure context、浏览器能不能 listen、mixed content 与私网 IP 豁免、
Rust Stream → JS ReadableStream、wasm-bindgen 边界。**Web 平台知识视角。**

### 5. [wasm-debugging/](wasm-debugging/) — 一个 bug 的十一轮调试
浏览器数据面静默——从「native 全绿 + 编译全过 + 控制面全通」到逐字节一致，
四道 wasm 运行时门逐层剥开的完整复盘 + 调试方法论。**调试实战视角。**

### 6. [transfer/](transfer/) — 传输域两篇专论
删掉整层 XChaCha20 为什么不降安全（自引用的冗余，把归属校验从隐式改为显式）、bao-tree
凭什么能边收边验（能力全在 BLAKE3 的二叉树结构里，SHA256 给不了）。与 `transfer-architecture/`
同题不同视角——后者讲分层与依赖倒置，本系列钻**安全模型与哈希结构**。**安全与结构视角。**

## 阅读顺序建议

- 想懂**架构决策**：1 → 3
- 想做 **Rust wasm 化**：2 → 4 → 5
- 想抠**传输安全 / 完整性**：6（配合 3 读，同题不同视角）
- 只想看**踩坑复盘**：5（自带前情，可独立读）

## 与旧文的关系

重构**前**的旧文（`end-to-end-encryption.md`、`transfer-protocol-design.md` 等，描述 XChaCha20
加密、旧 wire、单 crate）已归档到 [`../archive/pre-refactor-blogs/`](../archive/pre-refactor-blogs/)。
重构后的形态由 `transfer-architecture/`（软件设计视角）与 `transfer/`（安全与结构视角）反映，
相关篇会注明取代关系。
