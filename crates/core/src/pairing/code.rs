use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use swarm_p2p_core::libp2p::Multiaddr;

use crate::device::OsInfo;

const CHARSET: &[u8] = b"0123456789";
const CODE_LENGTH: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeInfo {
    pub code: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl PairingCodeInfo {
    pub fn generate(expires_in_secs: u64) -> Self {
        let mut rng = rand::rng();
        let code: String = (0..CODE_LENGTH)
            .map(|_| *CHARSET.choose(&mut rng).unwrap() as char)
            .collect();
        let now = chrono::Utc::now().timestamp();
        Self {
            code,
            created_at: now,
            expires_at: now + expires_in_secs as i64,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCodeRecord {
    #[serde(flatten)]
    pub os_info: OsInfo,
    pub created_at: i64,
    pub expires_at: i64,
    /// 发布者的可达地址，用于跨网络场景下让对方直接 dial。
    #[serde(default)]
    pub listen_addrs: Vec<Multiaddr>,
}

impl From<&PairingCodeInfo> for ShareCodeRecord {
    fn from(info: &PairingCodeInfo) -> Self {
        Self {
            created_at: info.created_at,
            expires_at: info.expires_at,
            os_info: OsInfo::default(),
            listen_addrs: Vec::new(),
        }
    }
}

/// 在线宣告记录，发布到 DHT 供已配对设备发现地址。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineRecord {
    #[serde(flatten)]
    pub os_info: OsInfo,
    #[serde(default)]
    pub listen_addrs: Vec<Multiaddr>,
    pub timestamp: i64,
}
