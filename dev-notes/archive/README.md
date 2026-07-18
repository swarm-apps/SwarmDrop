# 归档文档（archive）

历史文档存放处——**已完成、已被推翻或一次性的调研材料**，从 dev-notes 各活目录移入，
保留历史价值但不再是当前有效参考。找当前架构/设计请回到 `dev-notes/knowledge/`、
`dev-notes/architecture/`、`dev-notes/blogs/`。

> 归档时间：2026-07（网络内核重构后的一次整理）。均为 `git mv` 移动，历史可 `git log --follow` 追溯。

## 子目录

### [pre-refactor-design/](pre-refactor-design/) — 重构前的设计文档（2026-02）
早期传输/配对/MCP/认证设计，**大部分已被 2026-07 网络内核重构推翻**（wire v2、
删应用层加密、transfer 独立 crate、bao 逐块验证）。当前传输架构见
`blogs/transfer-architecture/`，当前实体设计见 `crates/entity` + migration。
- auth-design / mcp-design / mcp-server-implementation
- file-transfer-design / transfer-scenarios-design / transfer-features-analysis / transfer-system-refactor
- pairing-transfer-design / pairing-implementation / database-entity-design / mobile-desogn

### [completed-roadmap/](completed-roadmap/) — 已完成的实现路线图
Phase 1–4 全部完成（Networking / Pairing / File Transfer / Mobile），路线图作为历史记录归档。
- implementation-roadmap + phase-1~4

### [early-research/](early-research/) — 早期调研（2026-01）
移动端策略与 libp2p 可行性的最初调研，已被后续实践与 `knowledge/libp2p-wasm.md` 取代。
- mobile-strategy / mobile-libp2p-investigation

### [pre-refactor-blogs/](pre-refactor-blogs/) — 重构前的传输博客系列（5 篇）
早期的传输实现细节教程，描述**重构前**的形态（单 crate、XChaCha20 加密、拉取式旧 wire、
旧的分块/进度/并发实现）。架构决策层见 `blogs/transfer-architecture/`；具体代码/协议/文件路径
已随 transfer crate 化 + wire v2 + bao + FileAccess 端口全部变化。
- end-to-end-encryption（加密已删）/ transfer-protocol-design（旧 wire）
- concurrent-pulling-and-session-management / file-chunking-and-cross-platform-io / progress-tracking-and-frontend-state

### [recon-2026-07/](recon-2026-07/) — 重构期一次性调研与已落地规划
本次重构的输入材料与规划，重构完成后成为历史。当前结论已沉淀进
`knowledge/{net-kernel,libp2p-wasm,iroh-migration,storage-abstraction}.md` 与 `blogs/`。
- iroh-web-cli-recon / rendezvous-recon / direction-research-2026-07
- iroh-cross-platform-context / core-extraction-inventory（重构规划，已落地）

## 未归档的相关文档（仍活跃）
- `dev-notes/why-libp2p-not-iroh.md` — 决策文档，仍是当前架构的依据
- `dev-notes/architecture/iroh-invite-link-pairing-design.md` — PairInvite 设计，**待实施**（非废弃）
- `dev-notes/knowledge/*` — 全部活跃（dev-workflow 索引）
