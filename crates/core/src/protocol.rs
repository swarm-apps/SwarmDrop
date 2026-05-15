//! 应用层协议类型。

use serde::{Deserialize, Serialize};
use swarm_p2p_core::NetClient;
use uuid::Uuid;

use crate::device::OsInfo;

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

/// 文件校验和。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileChecksum {
    pub file_id: u32,
    pub checksum: String,
}

/// 断点续传被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResumeRejectReason {
    FileModified,
    SessionNotFound,
    SenderCancelled,
}

/// 传输请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TransferRequest {
    Offer {
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
    },
    ChunkRequest {
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
    },
    Complete {
        session_id: Uuid,
    },
    Cancel {
        session_id: Uuid,
        reason: String,
    },
    Pause {
        session_id: Uuid,
    },
    ResumeRequest {
        session_id: Uuid,
        file_checksums: Vec<FileChecksum>,
    },
    ResumeOffer {
        session_id: Uuid,
        #[serde(serialize_with = "serialize_key", deserialize_with = "deserialize_key")]
        key: [u8; 32],
        file_checksums: Vec<FileChecksum>,
    },
}

/// Offer 被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OfferRejectReason {
    NotPaired,
    UserDeclined,
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
    Chunk {
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        is_last: bool,
    },
    Ack {
        session_id: Uuid,
    },
    ChunkError {
        session_id: Uuid,
        file_id: u32,
        chunk_index: u32,
        error: String,
    },
    ResumeResult {
        session_id: Uuid,
        accepted: bool,
        reason: Option<ResumeRejectReason>,
        #[serde(
            serialize_with = "serialize_opt_key",
            deserialize_with = "deserialize_opt_key"
        )]
        key: Option<[u8; 32]>,
    },
    ResumeOfferResult {
        session_id: Uuid,
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
