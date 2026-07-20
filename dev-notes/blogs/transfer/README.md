# transfer — 传输域两篇专论

> 从**安全与结构**视角把两个删 / 换决定讲透：为什么删掉整层 XChaCha20 不降低安全，
> 以及接手完整性的 bao-tree 逐块验证凭什么能边收边验。
>
> 这是 [SwarmDrop 网络内核重构系列](../2026-07-net-refactor-series.md) 的第 6 个子系列。
> 传输域的分层与依赖倒置看 `transfer-architecture/`，本系列只钻这两篇的**为什么**。

## 篇目

| # | 标题 | 一句话 |
|---|---|---|
| [00](00-remove-xchacha20-crypto.md) | 删掉 XChaCha20：在已加密信道上再加密，是自引用的冗余 | Noise 已端到端加密的信道上再套一层 AEAD 是纯冗余，密钥还走同一条已加密信道分发——删它威胁模型一字未变；把加密隐式承担的数据面归属校验改成显式核对 `stream.remote() == session.peer_id` |
| [01](01-bao-tree-per-chunk-verify.md) | bao-tree 逐块验证：为什么 BLAKE3 的树能边收边验，SHA256 不能 | verified streaming 的能力全在 BLAKE3 的二叉 Merkle 树结构里——每收 256 KiB 块凭 outboard 立即验签、坏块当场发现、续传信任本地已验块；SHA256 的线性 Merkle–Damgård 链天生给不了 |

## 篇间关系

00 论证删掉加密后完整性要**显式承担**，01 讲承担它的引擎到底怎么转——两篇一前一后，正是
「删一层冗余、补一层逐块验签」的完整交代。

## 与 transfer-architecture/ 的分工

`transfer-architecture/` 从**软件设计**视角覆盖同两个决定
（[04 bao](../transfer-architecture/04-bao-tree-verified-streaming.md)、
[05 删加密](../transfer-architecture/05-removing-encryption-layer.md)），讲的是「放在哪一层、
依赖怎么倒置」；本系列换**安全模型与哈希结构**视角，讲「为什么删不降安全、为什么这棵树能边收边验」。
两边互为补充、不重复。

## 素材出处

- 删除前的加密实现：`crates/core/src/transfer/wire/crypto.rs`（提交 `5ce6be38` 整文件移除）
- bao-tree / outboard / verified streaming：`.claude/skills/iroh/references/03c-blobs.md`、
  `dev-notes/knowledge/iroh-migration.md`
- 数据面归属校验、wire v2 契约：`dev-notes/knowledge/net-kernel.md`
