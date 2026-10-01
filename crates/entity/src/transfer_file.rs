use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 与 `file_id` 组成复合唯一键（`session_file`）：同一会话内文件序号不可重复。
    #[sea_orm(unique_key = "session_file")]
    pub session_id: Uuid,
    // 刻意**不加** `on_delete`：文件行的删除由应用层负责（`delete_session` 走
    // `cascade_delete`、`clear_all_history` 先删子行再删会话）。加一条 DB 级 CASCADE
    // 会让同一件事有两套机制，且与既有行为不是逐字等价。
    #[sea_orm(belongs_to, from = "session_id", to = "session_id")]
    pub session: HasOne<super::transfer_session::Entity>,
    /// 会话内文件 ID（来自协议层，从 0 递增）
    #[sea_orm(unique_key = "session_file")]
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    /// BLAKE3 校验和（hex，64 字符）
    pub checksum: String,
    /// 文件传输状态
    pub status: FileStatus,
    /// 已传输字节数（接收方用，断点时持久化）
    pub transferred_bytes: i64,
    /// 该文件的总 chunk 数
    pub total_chunks: i32,
    /// 已完成 chunk 的 bitmap（BLOB）。
    /// 每 bit 对应一个 chunk，bit 1 = 已接收。
    /// 长度 = ceil(total_chunks / 8) 字节。
    /// 仅接收方使用，发送方为空 vec。
    pub completed_chunks: Vec<u8>,
    /// 已完成 byte ranges（JSON）。
    ///
    /// 新数据面以 range 为 checkpoint 事实源；bitmap 仅作为旧拉取实现和过渡适配。
    pub completed_ranges: String,
    /// 云暂存已同步到磁盘；不代表云对象已发布。续传可跳过 P2P，仅重试发布。
    pub staged_complete: bool,
    /// 发送方源文件路径（direction=send 时有值）。
    /// 桌面端为绝对路径字符串，用于断点续传时重建 FileSource。
    pub source_path: Option<String>,
    /// 接收方文件的最终落盘位置（direction=receive 且已完成时有值），由
    /// `finalize_sink` 返回：桌面端为绝对路径，移动端为 file:// 或 SAF
    /// document URI。历史行为 NULL——收件箱落库时回退目录拼接推导。
    pub location: Option<crate::FileLocation>,
    /// 发送方 bao-tree post-order outboard（BLOB，direction=send 时有值）。
    ///
    /// 逐块验签的 Merkle 树，prepare 阶段与 checksum 同一遍构建（root **就是** checksum）。
    /// 持久化避免 resume 时重算（1GiB 文件 ≈ 256KiB，约 0.024%——chunk group 与 `CHUNK_SIZE`
    /// 对齐之前是 4MiB / 0.4%）。
    ///
    /// **可用性判据是长度不是空**：见 `swarmdrop_transfer::bao::is_outboard_usable`。
    /// chunk group 一变，旧记录的字节仍然「非空且看起来合法」，用 `is_empty()` 判会把它
    /// 喂进新树、每块验签失败且重算分支永不触发。历史/旧会话为 NULL——同样按源文件重算回存。
    pub outboard: Option<Vec<u8>>,
}

impl ActiveModelBehavior for ActiveModel {}

/// 单文件传输状态
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
pub enum FileStatus {
    Pending,
    Completed,
    Failed,
}
