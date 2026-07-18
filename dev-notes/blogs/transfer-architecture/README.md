# transfer-architecture — 传输域的架构抽象

> 从**软件设计**视角看 SwarmDrop 的文件传输域：网络给什么形状、传输业务怎么切成独立 crate、
> 依赖如何倒置、怎么补齐逐块验签、又为什么删掉一整层加密。
>
> 这是 [SwarmDrop 网络内核重构系列](../2026-07-net-refactor-series.md) 的第 3 个子系列（软件设计
> 视角）。网络内核本身看 `network-kernel/`，wasm 工程看 `rust-wasm/`。

## 一条主线

**让每一块代码待在它该待的层，依赖只朝一个方向流。** 六篇顺着这条线，从「网络给业务什么形状」
一路推到「加密应该放在哪一层」。

## 篇目（建议顺序阅读，篇篇承接）

| # | 标题 | 一句话 |
|---|---|---|
| [00](00-dumbpipe-shape.md) | dumbpipe 形状：网络只给裸管道，业务自持 | 网络给裸字节流 + typed RPC，业务自持帧/offer/续传——vs 吞整套 blob 栈的 sendme 形状 |
| [01](01-crate-extraction.md) | 传输域抽成独立 crate：六层分层 | 从「埋在 core」到独立 `swarmdrop-transfer`；单一职责成编译期约束；git rename 保历史 |
| [02](02-dependency-inversion-ports.md) | 依赖倒置：端口 trait 定义在消费方 | transfer 定义 `SessionStore`/`PeerDirectory`/`FileAccess`，core 实现注入；边界 grep 零 sea_orm |
| [03](03-event-cycle-breaking.md) | 打破事件循环依赖：TransferEventSink | `CoreEvent` 反向引用 transfer wire 类型 → 成环；发自有 `TransferEvent` + 适配器 1:1 映射解环 |
| [04](04-bao-tree-verified-streaming.md) | bao-tree 逐块验证：文件收完前每块可验 | 唯一真实能力差；「已在用 blake3」是陷阱；root==checksum、Approach B、坏块被拒 |
| [05](05-removing-encryption-layer.md) | 删掉应用层加密：加密应该在哪一层 | 应用层加密在 Noise 之下是冗余自引用；与逐块验签的张力；补归属校验 |

## 篇间关系

```mermaid
graph LR
    P00["00 dumbpipe 形状"] --> P01["01 抽 crate 六层"]
    P01 --> P02["02 依赖倒置端口"]
    P02 --> P03["03 解事件环"]
    P03 --> P04["04 bao 逐块验签"]
    P04 --> P05["05 删应用层加密"]
    P04 -. "root==明文blake3<br/>依赖" .-> P05
```

- 00 的「dumbpipe 形状」是所有后续分层的前提——正因为业务只依赖裸管道，它才**能**被切成独立 crate。
- 02 与 03 是同一个依赖倒置的两种动机：02 解耦第三方依赖（sea-orm），03 打破真实的循环依赖。
- 04 与 05 互为因果：逐块验签要求 `root == 明文 blake3`，而这与应用层加密不可共存——删加密是 04
  能成立的前提之一。

## 与旧文的关系

`../transfer/` 下的旧文描述的是重构**前**的架构（单 crate、XChaCha20 加密、旧 wire）：

- **[`../transfer/end-to-end-encryption.md`](../transfer/end-to-end-encryption.md) 已被本系列
  [05](05-removing-encryption-layer.md) 取代**——它描述的应用层加密层已在 wire v2 删除。旧文的
  XChaCha20/Poly1305 密码学讲解本身仍有参考价值，但那一层不再存在。
- `../transfer/transfer-protocol-design.md` 等其余旧文描述旧 wire 协议（拉取式、CBOR、加密密钥
  握手），与本系列的 wire v2（裸流帧、明文 + bao proof、无密钥）形态不同，阅读时注意版本差异。

## 素材出处

- 存储抽象决策：`dev-notes/knowledge/storage-abstraction.md`
- bao-tree / dumbpipe / sendme 调研：`dev-notes/knowledge/iroh-migration.md` +
  `.claude/skills/iroh/references/03c-blobs.md`
- 传输域源码：`crates/transfer/src/`；端口层：`crates/host/src/`；组合根接线：`crates/core/src/`
