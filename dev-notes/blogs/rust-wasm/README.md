# rust-wasm — Rust 编译到浏览器实战

> **wasm 工程视角。** 怎么让一份 Rust 代码同时编到 native 和 `wasm32-unknown-unknown`，
> 且两边都能真正跑起来——SwarmDrop「单核心包」落地的完整工程记录。

这是[网络内核重构系列](../2026-07-net-refactor-series.md)的第 2 个子系列。起点是"浏览器能不能
复用桌面那份 Rust 传输逻辑"，终点是浏览器里跑与桌面**字面同一份** `swarmdrop-transfer`、
OPFS 落盘逐字节一致。

## 篇目

| # | 标题 | 一句话 |
|---|---|---|
| [00](00-single-core-package.md) | 单核心包：浏览器与桌面同一份 Rust 逻辑 | 全系列地图。"同"到什么程度、零 cfg 硬约束、iroh 的 shared 核心范式 |
| [01](01-dual-target-engineering.md) | 双 target 工程：cfg alias + target 依赖表 | 按平台换实现用 **target 依赖表**，不用 feature 开关（会 unification 爆炸）；`cfg_aliases`、空壳 crate、`check-wasm.sh` |
| [02](02-n0-future-tokio-shim.md) | n0-future：tokio 的浏览器替身 | native 上就是 `pub use tokio::*`，wasm 上换 `spawn_local`+`web_time`；三类 API 能换/不用换/换不了；JoinSet shim 缺陷 |
| [03](03-libp2p-master-pitfalls.md) | 吃 libp2p git master 的坑 | crates.io 的 webrtc-direct 是坏的，被迫 pin rev `93c5059`；facade 一起切、`=0.4.58` pin、relay HOP、`NoAddressesInReservation` |
| [04](04-wasm-toolchain.md) | wasm 工具链的坑（文档不会告诉你） | Apple clang 编不了 ring、getrandom 双版本双开关、wasm-bindgen 版本一致、member profile 被忽略、体积 |
| [05](05-what-compiles-isnt-what-runs.md) | 「编过 ≠ 能用」：wasm 的隐形门（总纲） | **题眼**。绿灯零保证；四道运行时门；wasm 单线程 + Web 语义是编译期看不见的第三维 |
| [06](06-sea-orm-to-storage-sql.md) | sea-orm 从 core 摘到 storage-sql：core 编 wasm 的存储前置 | **补篇**。卡点不在 entity（纯结构体原样过）在 `DatabaseConnection`（拖 sqlx/tokio/mio/ring）；切割线划在连接层，core 只认 `SessionStore`/`InboxStore` 端口、SQL 实现下沉——core 从此零 sea_orm |
| [07](07-webrtc-opfs-disk.md) | WebRTC 流式 + OPFS 落盘：大文件不炸内存 | **补篇**。收大文件不能整个塞内存——OPFS `createWritable` 常驻 + 每 256 KiB positioned write 直写；SyncAccessHandle 实测无增益、多养一套写法被主动删掉 |

## 阅读顺序

- **想动手 wasm 化**：00 → 01 → 02 → 03 → 04，最后必读 05。
- **只想看最扎心的教训**：直接 05（自带前情，可独立读）。

## 与相邻系列的分工

本系列只讲"**怎么编到两个 target 且都能用**"这条工程主线。相关但不同视角的内容在兄弟系列，
本系列只做交叉引用、不重复：

- [network-kernel/](../network-kernel/) — 学 iroh 重构网络内核（**架构演进视角**）。为什么学 iroh
  不迁 iroh、Endpoint 门面、relay reservation 时序等。
- [transfer-architecture/](../transfer-architecture/) — 传输域抽独立 crate、依赖倒置端口 trait
  （**软件设计视角**）。本系列 00 篇的"零 cfg 共享层"如何靠端口注入实现。
- [browser-platform/](../browser-platform/) — OPFS、secure context、mixed content、ReadableStream
  （**Web 平台知识视角**）。本系列 05 篇门 4 的平台背景在这里讲透。
- [wasm-debugging/](../wasm-debugging/) — 一个数据面 bug 的十一轮调试（**调试实战视角**）。本系列
  05 篇四道门的完整逐层剥开复盘在这里。

## 核对过的关键事实

写作时逐条核对的项目真实配置（防止照旧文档/训练数据写空）：

- **libp2p git rev = `93c5059`**（根 `Cargo.toml` libp2p / libp2p-stream / libp2p-webrtc 三者同 pin；
  `identity`/`multiaddr` 反而走 crates.io `0.2` 让 master 树自解析，避免两个 `PeerId` 类型）。
- **target 依赖表三段式**（`crates/net/Cargo.toml`）：公共段 `tokio = ["sync","macros"]`；native 段
  `tcp/quic/dns/websocket/mdns` + `libp2p-webrtc`；wasm 段 `websocket-websys/webrtc-websys` +
  `getrandom 0.3/wasm_js`。
- **getrandom 双版本**：0.2 靠 libp2p 传递的 `js` feature；0.3 直接依赖 `wasm_js` feature +
  `.cargo/config.toml` 的 `--cfg getrandom_backend="wasm_js"` rustflag——配置注释原话"两者缺一不可"。
- **`wasm-bindgen-futures = "=0.4.58"`**（`crates/net` wasm 段 + `crates/web`）：master 的
  libp2p-swarm 精确 pin，不跟就 cargo 无解。
- **`cfg_aliases = "0.2"`** + 各 crate `build.rs` 定义 `wasm_browser: { all(target_family = "wasm",
  target_os = "unknown") }`；业务层零 cfg 是硬约束。
- **体积**：net-web-smoke 冒烟壳 1836 KB 裸 / 598 KB gzip（对照 iroh spike 849 KB gzip）；完整
  传输栈 `crates/web` gzip 约翻倍。

## 素材来源

- `dev-notes/knowledge/libp2p-wasm.md` — wasm 可行性、编译探针、四道运行时门
- `dev-notes/knowledge/net-kernel.md` — libp2p master pin `93c5059` 的坑、wasm 工程约定
- `crates/net/Cargo.toml`、`crates/net/build.rs`、`crates/web/Cargo.toml`、`.cargo/config.toml`、
  `scripts/check-wasm.sh` — 双 target 配置实物
- `spike/webrtc-direct-https/`、`spike/net-web-smoke/` — webrtc-direct 与浏览器冒烟实证
- `.claude/skills/iroh/references/` — n0-future 设计、iroh wasm 工具链参考
