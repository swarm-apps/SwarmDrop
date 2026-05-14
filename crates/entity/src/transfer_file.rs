use sea_orm::entity::prelude::*;

use crate::FileStatus;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub session_id: Uuid,
    #[sea_orm(belongs_to, from = "session_id", to = "session_id")]
    pub session: HasOne<super::transfer_session::Entity>,
    /// 会话内文件 ID（来自协议层，从 0 递增）
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
    /// 发送方源文件路径（direction=send 时有值）。
    /// 桌面端为绝对路径字符串，用于断点续传时重建 FileSource。
    pub source_path: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
