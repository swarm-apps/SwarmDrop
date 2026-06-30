//! 应用层协议类型。

use serde::{Deserialize, Serialize};
use swarm_p2p_core::NetClient;
use uuid::Uuid;

use crate::device::OsInfo;
use entity::TerminalReason;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub os_info: OsInfo,
    pub timestamp: i64,
    pub method: PairingMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PairingMethod {
    Code { code: String },
    Direct,
}

/// 配对被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PairingRefuseReason {
    UserRejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum PairingResponse {
    Success,
    Refused { reason: PairingRefuseReason },
}

/// 传输文件元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub checksum: String,
}

/// 传输发起来源：人在应用内发起，或 AI 代理经 MCP 发起。
///
/// 由发送方自报、承载于 Offer，供接收端展示与 inbox 来源派生——是信息性/UX 信号，
/// 不作接收端安全边界（真正的控制是发送端 `allow_mcp_send_to_device` 门控）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TransferOrigin {
    /// 用户在应用内手动发起。
    Human,
    /// AI 代理经 MCP 发起；`client` 为 MCP 客户端名（如 claude-desktop），不可得时为 None。
    Mcp { client: Option<String> },
}

impl TransferOrigin {
    /// 序列化为 DB 列存储的紧凑字符串：`human` / `mcp` / `mcp:<client>`。
    pub fn to_db_string(&self) -> String {
        match self {
            TransferOrigin::Human => "human".to_string(),
            TransferOrigin::Mcp { client: Some(c) } => format!("mcp:{c}"),
            TransferOrigin::Mcp { client: None } => "mcp".to_string(),
        }
    }

    /// 从 DB 列字符串解析；无法识别（含历史 NULL→`"human"`）时回退 `Human`。
    pub fn from_db_string(s: &str) -> Self {
        match s {
            "mcp" => TransferOrigin::Mcp { client: None },
            other => match other.strip_prefix("mcp:") {
                Some(client) => TransferOrigin::Mcp {
                    client: Some(client.to_string()),
                },
                None => TransferOrigin::Human,
            },
        }
    }

    /// 是否为 MCP / AI 代理来源。
    pub fn is_mcp(&self) -> bool {
        matches!(self, TransferOrigin::Mcp { .. })
    }
}

/// 断点续传被拒绝的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResumeRejectReason {
    Cancelled,
    FatalError,
    SourceModified,
    CheckpointInvalid,
    PeerUnavailable,
    SessionNotFound,
}

/// 待传 byte range（fetch_plan 元素，恢复探测协议用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRange {
    pub file_id: u32,
    pub offset: u64,
    pub length: u64,
}

/// 单文件 checkpoint（已完成 byte range 列表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCheckpoint {
    pub file_id: u32,
    pub completed_ranges: Vec<(u64, u64)>,
}

/// 恢复探测时对端报告的 phase（简化映射）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResumePhaseReport {
    NotFound,
    Active,
    Suspended,
    Terminal,
}

/// 恢复状态报告内容（ResumeProbe 的应答体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeReport {
    pub phase: ResumePhaseReport,
    pub epoch: i64,
    pub files: Vec<FileInfo>,
    pub checkpoint: Vec<FileCheckpoint>,
    pub source_fingerprint: Option<String>,
    pub terminal: bool,
    pub terminal_reason: Option<TerminalReason>,
}

/// 传输请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TransferRequest {
    Offer {
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        /// 发起来源（人工 / MCP 代理），由发送方自报。
        origin: TransferOrigin,
    },
    Cancel {
        session_id: Uuid,
        reason: String,
    },
    Pause {
        session_id: Uuid,
    },
    /// 恢复探测：发起方询问对端会话当前事实。
    ResumeProbe {
        session_id: Uuid,
    },
    /// 恢复提交：发起方确认恢复，携带新 epoch、传输密钥和 fetch_plan。
    ResumeCommit {
        session_id: Uuid,
        new_epoch: i64,
        #[serde(serialize_with = "serialize_key", deserialize_with = "deserialize_key")]
        key: [u8; 32],
        fetch_plan: Vec<FileRange>,
    },
}

/// Offer 被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OfferRejectReason {
    NotPaired,
    UserDeclined,
    PolicyRejected,
    /// 接收方处于全局「暂停接收」状态，婉拒新 offer。
    ReceivingPaused,
}

/// 传输响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TransferResponse {
    OfferResult {
        accepted: bool,
        #[serde(
            serialize_with = "serialize_opt_key",
            deserialize_with = "deserialize_opt_key"
        )]
        key: Option<[u8; 32]>,
        reason: Option<OfferRejectReason>,
    },
    Ack {
        session_id: Uuid,
    },
    /// 恢复状态报告（对 ResumeProbe 的应答）。
    ResumeStateReport {
        session_id: Uuid,
        report: ResumeReport,
    },
    /// 恢复确认（对 ResumeCommit 的应答）。
    ResumeAck {
        session_id: Uuid,
        new_epoch: i64,
        accepted: bool,
        reason: Option<ResumeRejectReason>,
    },
}

fn serialize_key<S: serde::Serializer>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(&key[..])
}

fn deserialize_key<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<[u8; 32], D::Error> {
    let v: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    v.try_into()
        .map_err(|_| serde::de::Error::custom("expected 32 bytes for key"))
}

fn serialize_opt_key<S: serde::Serializer>(
    key: &Option<[u8; 32]>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match key {
        Some(k) => serializer.serialize_some(&k[..]),
        None => serializer.serialize_none(),
    }
}

fn deserialize_opt_key<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<[u8; 32]>, D::Error> {
    let opt: Option<Vec<u8>> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(v) => {
            let arr: [u8; 32] = v
                .try_into()
                .map_err(|_| serde::de::Error::custom("expected 32 bytes for key"))?;
            Ok(Some(arr))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum AppRequest {
    Pairing(PairingRequest),
    Transfer(TransferRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum AppResponse {
    Pairing(PairingResponse),
    Transfer(TransferResponse),
}

pub type AppNetClient = NetClient<AppRequest, AppResponse>;

#[cfg(test)]
mod tests {
    use super::TransferOrigin;

    #[test]
    fn transfer_origin_db_string_roundtrip() {
        for origin in [
            TransferOrigin::Human,
            TransferOrigin::Mcp { client: None },
            TransferOrigin::Mcp {
                client: Some("claude-desktop".to_string()),
            },
        ] {
            let s = origin.to_db_string();
            assert_eq!(
                TransferOrigin::from_db_string(&s),
                origin,
                "roundtrip via {s}"
            );
        }
    }

    #[test]
    fn transfer_origin_from_db_string_fallback() {
        assert_eq!(
            TransferOrigin::from_db_string("human"),
            TransferOrigin::Human
        );
        // 未知 / 历史 NULL→"" 一律回退 Human，绝不 panic。
        assert_eq!(
            TransferOrigin::from_db_string("unknown"),
            TransferOrigin::Human
        );
        assert_eq!(TransferOrigin::from_db_string(""), TransferOrigin::Human);
        assert_eq!(
            TransferOrigin::from_db_string("mcp"),
            TransferOrigin::Mcp { client: None }
        );
        assert_eq!(
            TransferOrigin::from_db_string("mcp:cursor"),
            TransferOrigin::Mcp {
                client: Some("cursor".to_string())
            }
        );
    }
}
