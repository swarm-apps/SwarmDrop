use sea_orm::entity::prelude::*;

use crate::{PeerId, SaveLocation, SessionStatus, TransferDirection};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_sessions")]
pub struct Model {
    /// 会话 ID（UUID），来自协议层，收发双方各自独立记录
    #[sea_orm(primary_key, auto_increment = false)]
    pub session_id: Uuid,
    /// 传输方向
    pub direction: TransferDirection,
    /// 对端 libp2p PeerId（base58btc 字符串表示）
    #[sea_orm(column_type = "Text")]
    pub peer_id: PeerId,
    /// 对端设备名（快照，不跟踪更新）
    pub peer_name: String,
    /// 所有文件总字节数
    pub total_size: i64,
    /// 已传输字节数（实时更新）
    pub transferred_bytes: i64,
    /// 会话状态
    pub status: SessionStatus,
    /// 开始时间（Unix ms）
    pub started_at: i64,
    /// 最后更新时间（Unix ms），用于 paused 会话 7 天过期清理
    pub updated_at: i64,
    /// 完成/失败/取消时间（Unix ms），进行中为 NULL
    pub finished_at: Option<i64>,
    /// 失败原因（status=failed 时有值）
    pub error_message: Option<String>,
    /// 接收方保存位置（direction=receive 时有值）
    /// JSON 序列化的 SaveLocation 枚举
    pub save_path: Option<SaveLocation>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::transfer_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
