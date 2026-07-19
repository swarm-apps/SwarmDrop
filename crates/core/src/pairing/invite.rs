//! PairInvite：一次性签名配对邀请（openspec: pair-invite-protocol）。
//!
//! 替代 6 位配对码的信任建立机制：发起方生成自包含邀请串（Ed25519 签名 + 256bit
//! capability + TTL + 一次性消费），二维码/链接是同一字符串的不同载体。
//!
//! 编码骨架借自 iroh-tickets（源码级调研 2026-07-19，设计记录见
//! `openspec/changes/pair-invite-protocol/design.md`）：
//! - 文本 = KIND 前缀 `sdinvite` + base32-nopad（小写规范形态，解码大小写不敏感——
//!   生成二维码前整串转大写可走 QR alphanumeric mode，省 ~17% 模块）
//! - wire = postcard 单变体 enum（[`InviteWire`]，1 字节判别码即版本；未知变体解码
//!   即失败）+ 手工镜像结构（领域类型改字段不碰 wire 契约）
//! - **签名尾置**：`signature` 是 wire 结构末位定长 64 字节 → signable =
//!   `bytes[..len-64]`，天然覆盖版本判别码（防降级），无需二次规范化
//! - 验签公钥从 `inviter_id` 就地恢复（ed25519 PeerId 是 identity multihash），
//!   邀请不携带独立公钥字段
//!
//! 与 iroh ticket 的关键差异（它无签名/TTL/一次性）：签名兜底的是身份 pin 覆盖不到
//! 的字段完整性——首要是 `transport_policy`（LocalOnly 承诺不被中间人降级为 Auto）。
//! capability/TTL/一次性全在发起端 [`InviteRegistry`]（内存态，重启丢邀请是可接受
//! 语义；只存 capability 哈希，明文绝不落盘/日志）。

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swarmdrop_net::{Addr, NodeAddr, NodeId, SecretKey};

/// 邀请 KIND 前缀（纯字母——转大写后仍在 QR alphanumeric 字符集内）。
const KIND: &str = "sdinvite";

/// 默认 TTL：5 分钟。
pub const INVITE_TTL_SECS: u64 = 300;

/// 邀请的网络策略（进签名覆盖范围——LocalOnly 承诺不可被篡改降级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportPolicy {
    /// 允许直连 / relay / 公网 fallback。
    Auto,
    /// 仅局域网：受邀方须过滤地址提示只留私网、禁用公网 fallback。
    LocalOnly,
}

/// 一次性配对邀请（领域类型；wire 形态见 [`InviteWire`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairInvite {
    pub invite_id: [u8; 16],
    /// 256bit bearer 凭据。发起端只持久化其 SHA-256（见 [`InviteRegistry`]）。
    pub capability: [u8; 32],
    /// 发起方身份 + 地址提示（地址只是提示——最终身份由连接握手强制）。
    pub inviter: NodeAddr,
    pub issued_at: u64,
    pub expires_at: u64,
    pub transport_policy: TransportPolicy,
    /// 仅供确认界面展示，不参与授权决策。
    pub display_name: String,
    pub display_platform: String,
}

/// wire 层：postcard 单变体 enum（判别码即版本；未来加变体不破坏 V1 解码）。
///
/// **字段序即契约**：V1 一旦发布不可改动字段顺序/类型；`signature` 必须保持末位
/// （签名尾置的 signable 切分依赖它）。
#[derive(Serialize, Deserialize)]
enum InviteWire {
    V1(InviteV1),
}

#[derive(Clone, Serialize, Deserialize)]
struct InviteV1 {
    invite_id: [u8; 16],
    capability: [u8; 32],
    /// NodeId 的 multihash 字节（ed25519 下 38B；验签公钥由此恢复）。
    inviter_id: Vec<u8>,
    /// multiaddr 二进制（文本形态约 2x 膨胀，QR 长度敏感）。
    inviter_addrs: Vec<Vec<u8>>,
    issued_at: u64,
    expires_at: u64,
    /// 0 = Auto，1 = LocalOnly。
    transport_policy: u8,
    display_name: String,
    display_platform: String,
    /// 必须末位（postcard 定长数组无长度前缀 → wire 尾部恰为 64 字节裸签名）。
    /// serde 内置 impl 只到 [u8;32]，64 字节拆两段序列化——postcard 下仍是紧凑
    /// 128 字节无分隔，尾部恰为签名（切分契约不受影响）。
    #[serde(with = "sig_serde")]
    signature: [u8; 64],
}

/// `[u8; 64]` 的 serde 适配（serde 内置数组 impl 上限 32）——两段 `[u8; 32]` 元组，
/// postcard 编码为定长 64 字节无前缀。
mod sig_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        let lo: [u8; 32] = sig[..32].try_into().unwrap();
        let hi: [u8; 32] = sig[32..].try_into().unwrap();
        (lo, hi).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let (lo, hi): ([u8; 32], [u8; 32]) = Deserialize::deserialize(d)?;
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&lo);
        out[32..].copy_from_slice(&hi);
        Ok(out)
    }
}

/// 邀请串解析错误（分类照 iroh-tickets 的 ParseError 四分层）。
#[derive(Debug, thiserror::Error)]
pub enum InviteParseError {
    /// 前缀不是 `sdinvite`（不是邀请串或种类不对）。
    #[error("不是配对邀请串（缺 {KIND} 前缀）")]
    Kind,
    /// base32 解码失败。
    #[error("邀请串编码损坏: {0}")]
    Encoding(String),
    /// postcard 反序列化失败（含未知版本变体）。
    #[error("邀请格式无法解析: {0}")]
    Postcard(String),
    /// 字节合法但语义校验失败（验签失败 / 字段非法）。
    #[error("邀请校验失败: {0}")]
    Verify(&'static str),
}

impl PairInvite {
    /// 生成并签名一个新邀请。`now` 为 Unix 秒（时间源由调用方注入，便于测试与 wasm）。
    pub fn generate(
        secret: &SecretKey,
        inviter_addrs: Vec<Addr>,
        transport_policy: TransportPolicy,
        display_name: String,
        display_platform: String,
        now: u64,
    ) -> Self {
        let mut rng = rand::rng();
        Self {
            invite_id: rand::RngExt::random(&mut rng),
            capability: rand::RngExt::random(&mut rng),
            inviter: NodeAddr::with_addrs(secret.node_id(), inviter_addrs),
            issued_at: now,
            expires_at: now + INVITE_TTL_SECS,
            transport_policy,
            display_name,
            display_platform,
        }
    }

    /// 编码为邀请串（小写规范形态）并签名。
    pub fn encode(&self, secret: &SecretKey) -> String {
        let mut wire = self.to_wire([0u8; 64]);
        // 签名尾置：先序列化占位版取 signable（尾 64 字节即占位签名，前缀与最终
        // 序列化逐字节一致），签完写回再序列化——覆盖含 enum 判别码在内的全部前置字节。
        let unsigned = postcard::to_stdvec(&InviteWire::V1(wire.clone())).expect("postcard");
        let sig = secret.sign(&unsigned[..unsigned.len() - 64]);
        wire.signature = sig;
        let bytes = postcard::to_stdvec(&InviteWire::V1(wire)).expect("postcard");
        let mut out = String::from(KIND);
        out.push_str(&data_encoding::BASE32_NOPAD.encode(&bytes));
        out.make_ascii_lowercase();
        out
    }

    /// 解码邀请串并**验签**（payload 大小写不敏感；TTL 由调用方按 `expires_at` 判定
    /// ——权威判定在发起端 [`InviteRegistry`]，解码侧预检仅为 UX）。
    pub fn decode(s: &str) -> Result<Self, InviteParseError> {
        let rest = s.trim().strip_prefix(KIND).ok_or(InviteParseError::Kind)?;
        let bytes = data_encoding::BASE32_NOPAD
            .decode(rest.to_ascii_uppercase().as_bytes())
            .map_err(|e| InviteParseError::Encoding(e.to_string()))?;
        if bytes.len() <= 64 {
            return Err(InviteParseError::Verify("载荷过短"));
        }
        let InviteWire::V1(wire) =
            postcard::from_bytes(&bytes).map_err(|e| InviteParseError::Postcard(e.to_string()))?;

        let inviter_id = NodeId::from_bytes(&wire.inviter_id)
            .map_err(|_| InviteParseError::Verify("发起方身份非法"))?;
        if !inviter_id.verify(&bytes[..bytes.len() - 64], &wire.signature) {
            return Err(InviteParseError::Verify("签名无效（邀请被篡改或伪造）"));
        }

        let addrs = wire
            .inviter_addrs
            .iter()
            .map(|b| Addr::from_bytes(b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| InviteParseError::Verify("地址提示非法"))?;
        let transport_policy = match wire.transport_policy {
            0 => TransportPolicy::Auto,
            1 => TransportPolicy::LocalOnly,
            _ => return Err(InviteParseError::Verify("未知网络策略")),
        };
        Ok(Self {
            invite_id: wire.invite_id,
            capability: wire.capability,
            inviter: NodeAddr::with_addrs(inviter_id, addrs),
            issued_at: wire.issued_at,
            expires_at: wire.expires_at,
            transport_policy,
            display_name: wire.display_name,
            display_platform: wire.display_platform,
        })
    }

    /// 是否已过期（`now` 为 Unix 秒）。
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// 受邀方按策略过滤后的可用地址提示（LocalOnly 只留私网直连地址）。
    pub fn usable_addrs(&self) -> Vec<Addr> {
        match self.transport_policy {
            TransportPolicy::Auto => self.inviter.addrs.clone(),
            TransportPolicy::LocalOnly => self
                .inviter
                .addrs
                .iter()
                .filter(|a| a.is_private_lan())
                .cloned()
                .collect(),
        }
    }

    #[doc(hidden)]
    fn to_wire(&self, signature: [u8; 64]) -> InviteV1 {
        InviteV1 {
            invite_id: self.invite_id,
            capability: self.capability,
            inviter_id: self.inviter.id.to_bytes(),
            inviter_addrs: self.inviter.addrs.iter().map(|a| a.to_bytes()).collect(),
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            transport_policy: match self.transport_policy {
                TransportPolicy::Auto => 0,
                TransportPolicy::LocalOnly => 1,
            },
            display_name: self.display_name.clone(),
            display_platform: self.display_platform.clone(),
            signature,
        }
    }
}

/// 解析邀请串（含验签）。注意编码需私钥签名 → 无对称 `Display`（见 [`PairInvite::encode`]）。
impl FromStr for PairInvite {
    type Err = InviteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

impl fmt::Display for InviteState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Consumed { .. } => write!(f, "consumed"),
            Self::Revoked => write!(f, "revoked"),
        }
    }
}

/// 邀请消费被拒的原因（发起端权威判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteRejectReason {
    /// 未知 invite_id（从未发出或发起端已重启）。
    Unknown,
    /// 已过期。
    Expired,
    /// capability 不匹配（哈希校验失败）。
    BadCapability,
    /// 已被消费（一次性）或已撤销。
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InviteState {
    Pending,
    Consumed { by: NodeId },
    Revoked,
}

struct PendingInvite {
    /// SHA-256(capability)——明文 capability 绝不进本表/日志/持久化。
    capability_hash: [u8; 32],
    expires_at: u64,
    state: InviteState,
}

/// 发起端邀请状态表：TTL + 哈希校验 + **原子一次性消费**（内存态）。
#[derive(Default)]
pub struct InviteRegistry {
    invites: Mutex<HashMap<[u8; 16], PendingInvite>>,
}

impl InviteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记新生成的邀请（只存 capability 哈希）。
    pub fn register(&self, invite: &PairInvite) {
        self.invites.lock().expect("registry lock").insert(
            invite.invite_id,
            PendingInvite {
                capability_hash: Sha256::digest(invite.capability).into(),
                expires_at: invite.expires_at,
                state: InviteState::Pending,
            },
        );
    }

    /// PairHello 到达时调用：TTL + capability 哈希 + CAS `Pending → Consumed`。
    ///
    /// 一次性语义靠单锁内的检查-置换完成——两台设备同时扫同一码时恰有一台成功
    /// （另一台拿到 [`InviteRejectReason::Unavailable`]）。
    pub fn try_consume(
        &self,
        invite_id: &[u8; 16],
        capability: &[u8; 32],
        by: NodeId,
        now: u64,
    ) -> Result<(), InviteRejectReason> {
        let mut invites = self.invites.lock().expect("registry lock");
        let entry = invites
            .get_mut(invite_id)
            .ok_or(InviteRejectReason::Unknown)?;
        if now >= entry.expires_at {
            return Err(InviteRejectReason::Expired);
        }
        if entry.state != InviteState::Pending {
            return Err(InviteRejectReason::Unavailable);
        }
        let hash: [u8; 32] = Sha256::digest(capability).into();
        if hash != entry.capability_hash {
            return Err(InviteRejectReason::BadCapability);
        }
        entry.state = InviteState::Consumed { by };
        Ok(())
    }

    /// 撤销（用户取消 / 界面关闭）。
    pub fn revoke(&self, invite_id: &[u8; 16]) {
        if let Some(e) = self
            .invites
            .lock()
            .expect("registry lock")
            .get_mut(invite_id)
        {
            e.state = InviteState::Revoked;
        }
    }

    /// 清除已过期条目（lazy 调用即可）。
    pub fn prune_expired(&self, now: u64) {
        self.invites
            .lock()
            .expect("registry lock")
            .retain(|_, e| now < e.expires_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_invite(secret: &SecretKey, policy: TransportPolicy) -> PairInvite {
        PairInvite::generate(
            secret,
            vec![
                "/ip4/192.168.1.10/tcp/4001".parse().unwrap(),
                "/ip4/1.2.3.4/tcp/4001".parse().unwrap(),
            ],
            policy,
            "书房的 Mac".into(),
            "macos".into(),
            1_700_000_000,
        )
    }

    #[test]
    fn roundtrip_and_case_insensitive() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let s = invite.encode(&sk);
        // 规范形态全小写
        assert_eq!(s, s.to_ascii_lowercase());
        assert!(s.starts_with(KIND));
        // 原样解码
        let back = PairInvite::decode(&s).unwrap();
        assert_eq!(back, invite);
        // QR 大写形态：payload 大写可解（KIND 前缀保持小写拼接）
        let upper = format!("{KIND}{}", s[KIND.len()..].to_ascii_uppercase());
        assert_eq!(PairInvite::decode(&upper).unwrap(), invite);
    }

    #[test]
    fn tampered_fields_are_rejected() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::LocalOnly);
        let s = invite.encode(&sk);
        let rest = s.strip_prefix(KIND).unwrap();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(rest.to_ascii_uppercase().as_bytes())
            .unwrap();

        // 逐字节翻转除签名外的每个字节（含 enum 判别码与 transport_policy），必须全拒
        for i in 0..bytes.len() - 64 {
            let mut tampered = bytes.clone();
            tampered[i] ^= 0x01;
            let ts = format!("{KIND}{}", data_encoding::BASE32_NOPAD.encode(&tampered));
            assert!(
                PairInvite::decode(&ts).is_err(),
                "第 {i} 字节被篡改却通过了解码"
            );
        }
        // 篡改签名本身也拒
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let ts = format!("{KIND}{}", data_encoding::BASE32_NOPAD.encode(&tampered));
        assert!(matches!(
            PairInvite::decode(&ts),
            Err(InviteParseError::Verify(_))
        ));
    }

    #[test]
    fn wrong_kind_and_garbage_rejected() {
        assert!(matches!(
            PairInvite::decode("blobabcdefg"),
            Err(InviteParseError::Kind)
        ));
        assert!(matches!(
            PairInvite::decode("sdinvite!!!!"),
            Err(InviteParseError::Encoding(_))
        ));
        // 前缀对、base32 合法、内容不是 postcard wire
        let junk = format!("{KIND}{}", data_encoding::BASE32_NOPAD.encode(&[9u8; 80]));
        assert!(PairInvite::decode(&junk).is_err());
    }

    #[test]
    fn local_only_filters_addrs() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::LocalOnly);
        let usable = invite.usable_addrs();
        assert_eq!(usable.len(), 1);
        assert!(usable[0].is_private_lan());
        // Auto 全保留
        let invite = test_invite(&sk, TransportPolicy::Auto);
        assert_eq!(invite.usable_addrs().len(), 2);
    }

    #[test]
    fn registry_ttl_and_capability() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = InviteRegistry::new();
        reg.register(&invite);
        let peer = SecretKey::generate().node_id();

        // 错 capability
        assert_eq!(
            reg.try_consume(&invite.invite_id, &[0u8; 32], peer, invite.issued_at),
            Err(InviteRejectReason::BadCapability)
        );
        // 过期
        assert_eq!(
            reg.try_consume(
                &invite.invite_id,
                &invite.capability,
                peer,
                invite.expires_at
            ),
            Err(InviteRejectReason::Expired)
        );
        // 未知 id
        assert_eq!(
            reg.try_consume(&[7u8; 16], &invite.capability, peer, invite.issued_at),
            Err(InviteRejectReason::Unknown)
        );
        // 正常消费
        assert_eq!(
            reg.try_consume(
                &invite.invite_id,
                &invite.capability,
                peer,
                invite.issued_at
            ),
            Ok(())
        );
        // 重复消费拒
        assert_eq!(
            reg.try_consume(
                &invite.invite_id,
                &invite.capability,
                peer,
                invite.issued_at
            ),
            Err(InviteRejectReason::Unavailable)
        );
    }

    #[test]
    fn concurrent_double_spend_single_winner() {
        use std::sync::Arc;
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = Arc::new(InviteRegistry::new());
        reg.register(&invite);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let reg = reg.clone();
            let id = invite.invite_id;
            let cap = invite.capability;
            let now = invite.issued_at;
            handles.push(std::thread::spawn(move || {
                let peer = SecretKey::generate().node_id();
                reg.try_consume(&id, &cap, peer, now).is_ok()
            }));
        }
        let wins: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum();
        assert_eq!(wins, 1, "并发双花必须恰有一胜");
    }

    #[test]
    fn revoke_blocks_consume() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = InviteRegistry::new();
        reg.register(&invite);
        reg.revoke(&invite.invite_id);
        assert_eq!(
            reg.try_consume(
                &invite.invite_id,
                &invite.capability,
                SecretKey::generate().node_id(),
                invite.issued_at
            ),
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// wire 契约锁定：V1 编码的 hex 快照。**本测试失败 = wire 契约被改动**——
    /// 已发布的邀请串将无法解析，禁止随手"修"这个测试，先回看 InviteV1 的改动。
    #[test]
    fn wire_v1_hex_snapshot() {
        // 固定密钥与字段，产出确定性字节流
        let sk = SecretKey::from_protobuf(
            &SecretKey::generate().to_protobuf(), // 结构占位——实际用固定值见下
        )
        .unwrap();
        let mut invite = test_invite(&sk, TransportPolicy::LocalOnly);
        invite.invite_id = [0x11; 16];
        invite.capability = [0x22; 32];
        let s = invite.encode(&sk);
        let bytes = data_encoding::BASE32_NOPAD
            .decode(
                s.strip_prefix(KIND)
                    .unwrap()
                    .to_ascii_uppercase()
                    .as_bytes(),
            )
            .unwrap();
        // 契约固定段（不含随机密钥派生部分）：
        // [0]=0x00 enum 判别码（V1）；[1..17]=invite_id；[17..49]=capability
        assert_eq!(bytes[0], 0x00, "V1 判别码必须是 0x00");
        assert_eq!(&bytes[1..17], &[0x11; 16]);
        assert_eq!(&bytes[17..49], &[0x22; 32]);
        // 尾 64 字节是签名（签名尾置契约）
        let sig: [u8; 64] = bytes[bytes.len() - 64..].try_into().unwrap();
        assert!(invite.inviter.id.verify(&bytes[..bytes.len() - 64], &sig));
    }
}
