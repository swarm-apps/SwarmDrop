//! 数据面层：帧编解码 + data channel 路由。
//!
//! - [`data_frame`] —— `TransferDataFrame` 编解码 + manifest digest
//! - [`data_plane`] —— data channel 入站/出站路由到 actor（纯路由 + 注册表簿记）
//!
//! wire v2 删除了应用层分块加密（`crypto` 整文件移除）：Noise/TLS 在途已加密，
//! relay 只见密文，密钥经同一加密信道分发是自引用——数据面直接传明文。
//! 传输层身份即归属证明：数据面 handler 校验 `stream.remote() == session.peer_id`。
//!
//! ## 逐块验签（bao-tree，见 [`crate::bao`]）
//!
//! `BlockData.proof` 已启用（不再恒 `None`）：每个块携带 bao-tree 切片，接收端**在文件收完
//! 前**逐块验证——取代「续传信任对端」的现状。选型两条决策记此：
//!
//! - **proof 携完整 bao 切片、`data` 置空**（Approach B）：库没有稳定的「拆 Parent/Leaf 交错流」
//!   公开迭代顺序 API，手动交错易错；完整切片方案 data 置空后叶子只出现一次、无 2x 冗余
//!   （开销 ≈ 明文 + parents ≈ 0.4%）。proof 是 opaque bytes，wire 布局不变。proof 缺失
//!   （`None`）或验签失败 = 协议违规 → 断流走既有 Interrupted 恢复（v2 两端同步发布，无渐进
//!   兼容需求，发送端恒带 proof）。
//! - **接收端不建 outboard**：我们不做再分发（裁掉调研里的 PostOrderOutboard 场景）。逐块 proof
//!   验证通过才 `mark_chunk_completed`，故 checkpoint bitmap 本身可信；**resume 时信任本地磁盘**
//!   （本地篡改不在传输威胁模型内）。发送端 outboard 与 checksum 同一遍构建、随会话落库
//!   （`transfer_files.outboard`），resume 免重算（缺失则按源文件重算回存）。

pub mod data_frame;
pub(crate) mod data_plane;

pub use data_plane::TransferDataHandler;
