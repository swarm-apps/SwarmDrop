use sha2::Digest;
use swarm_p2p_core::libp2p::kad::RecordKey;

const NS_SHARE_CODE: &[u8] = b"/swarmdrop/share-code/";
const NS_ONLINE: &[u8] = b"/swarmdrop/online/";

fn dht_key(namespace: &[u8], id: &[u8]) -> RecordKey {
    sha2::Sha256::digest([namespace, id].concat())
        .to_vec()
        .into()
}

/// 配对码的 DHT key。
pub fn share_code_key(code: &str) -> RecordKey {
    dht_key(NS_SHARE_CODE, code.as_bytes())
}

/// 在线宣告的 DHT key。
pub fn online_key(peer_id_bytes: &[u8]) -> RecordKey {
    dht_key(NS_ONLINE, peer_id_bytes)
}
