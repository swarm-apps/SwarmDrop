//! 节点身份：[`NodeId`]（公开标识）与 [`SecretKey`]（私钥）。

use std::fmt;
use std::str::FromStr;

use libp2p_identity::{Keypair, PeerId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 身份与密钥的解析错误。
#[derive(Debug, thiserror::Error)]
pub enum IdParseError {
    /// base58 字符串不是合法的节点标识。
    #[error("invalid node id: {0}")]
    InvalidNodeId(String),
    /// 字节串不是合法的 protobuf 编码密钥。
    #[error("invalid secret key: {0}")]
    InvalidSecretKey(String),
}

/// 节点身份标识（ed25519 公钥派生）。
///
/// 字符串表示为 base58，与旧栈 `libp2p::PeerId` 的表示完全一致——
/// 数据库与 keychain 中既有的 peer_id 字符串无需迁移。
/// serde 序列化为字符串（进 IPC/FFI/DHT record 都是这个形态）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(PeerId);

impl NodeId {
    /// 从 base58 字符串解析。
    pub fn from_base58(s: &str) -> Result<Self, IdParseError> {
        PeerId::from_str(s)
            .map(Self)
            .map_err(|e| IdParseError::InvalidNodeId(e.to_string()))
    }

    /// 内核内部互转用，业务层不应调用。
    #[doc(hidden)]
    pub fn from_peer_id(peer_id: PeerId) -> Self {
        Self(peer_id)
    }

    /// 内核内部互转用，业务层不应调用。
    #[doc(hidden)]
    pub fn as_peer_id(&self) -> &PeerId {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

impl FromStr for NodeId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_base58(s)
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_base58(&s).map_err(serde::de::Error::custom)
    }
}

/// 节点私钥（ed25519）。
///
/// protobuf 编码与旧栈 `Keypair::{from,to}_protobuf_encoding` 完全一致——
/// keychain / Stronghold 中的存量密钥直接可用。
#[derive(Clone)]
pub struct SecretKey(Keypair);

impl SecretKey {
    /// 生成新的 ed25519 密钥。
    pub fn generate() -> Self {
        Self(Keypair::generate_ed25519())
    }

    /// 从 protobuf 编码恢复（keychain 存量格式）。
    pub fn from_protobuf(bytes: &[u8]) -> Result<Self, IdParseError> {
        Keypair::from_protobuf_encoding(bytes)
            .map(Self)
            .map_err(|e| IdParseError::InvalidSecretKey(e.to_string()))
    }

    /// 导出 protobuf 编码（keychain 存量格式）。
    pub fn to_protobuf(&self) -> Vec<u8> {
        self.0
            .to_protobuf_encoding()
            .expect("ed25519 keypair is always protobuf-encodable")
    }

    /// 对应的公开节点标识。
    pub fn node_id(&self) -> NodeId {
        NodeId(self.0.public().to_peer_id())
    }

    /// 内核内部互转用，业务层不应调用。
    #[doc(hidden)]
    pub fn as_keypair(&self) -> &Keypair {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 绝不打印密钥材料
        write!(f, "SecretKey({})", self.node_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_base58_roundtrip_matches_peer_id() {
        let sk = SecretKey::generate();
        let id = sk.node_id();
        // base58 表示与 libp2p PeerId 完全一致（存量 DB 兼容的根据）
        let peer_id_str = sk.0.public().to_peer_id().to_string();
        assert_eq!(id.to_string(), peer_id_str);
        // roundtrip
        let parsed = NodeId::from_base58(&id.to_string()).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn secret_key_protobuf_roundtrip() {
        let sk = SecretKey::generate();
        let bytes = sk.to_protobuf();
        let restored = SecretKey::from_protobuf(&bytes).unwrap();
        assert_eq!(restored.node_id(), sk.node_id());
    }

    #[test]
    fn node_id_serde_is_string() {
        let id = SecretKey::generate().node_id();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn invalid_base58_is_rejected() {
        assert!(NodeId::from_base58("not-a-node-id").is_err());
        assert!(NodeId::from_base58("").is_err());
        assert!(serde_json::from_str::<NodeId>("\"bogus\"").is_err());
    }

    #[test]
    fn invalid_protobuf_is_rejected() {
        assert!(SecretKey::from_protobuf(b"garbage").is_err());
        assert!(SecretKey::from_protobuf(&[]).is_err());
    }

    #[test]
    fn secret_key_debug_never_prints_key_material() {
        let sk = SecretKey::generate();
        let debug = format!("{sk:?}");
        // Debug 只允许出现公开的 NodeId，不允许出现任何私钥字节的痕迹
        assert!(debug.contains(&sk.node_id().to_string()));
        let proto_hex: String = sk.to_protobuf()[4..8] // 取私钥体的片段
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(
            !debug.to_lowercase().contains(&proto_hex),
            "Debug 输出疑似包含私钥材料"
        );
    }
}
