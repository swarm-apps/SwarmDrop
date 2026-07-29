//! PairInvite：一次性签名配对邀请（openspec: pair-invite-protocol）。
//!
//! 替代 6 位配对码的信任建立机制：发起方生成自包含邀请串（Ed25519 签名 + 128bit
//! capability + TTL + 一次性消费），二维码/链接是同一字符串的不同载体。
//!
//! 编码骨架借自 iroh-tickets（源码级调研 2026-07-19，设计记录见
//! `openspec/changes/pair-invite-protocol/design.md`）：
//! - 链接文本 = KIND 前缀 `sd:` + base64url-nopad（URL 安全、比 base32 短）
//! - 二维码文本 = KIND 前缀 `SD` + base32-nopad（二维码专用；走 alphanumeric mode）。
//!   两种文本承载同一份 wire，统一由 [`PairInvite::decode`] 验签。
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
use std::str::FromStr;
use std::sync::Mutex;

use data_encoding::{BASE32_NOPAD, BASE64URL_NOPAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swarmdrop_net_base::{Addr, NodeAddr, NodeId, SecretKey};

/// 邀请 KIND 前缀。二维码形态去掉 `:` 后统一大写，仍在 QR alphanumeric 字符集内。
const KIND: &str = "sd";

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
    /// 128bit bearer 凭据。发起端只持久化其 SHA-256（见 [`InviteRegistry`]）。
    pub capability: [u8; 16],
    /// 发起方身份 + 地址提示（地址只是提示——最终身份由连接握手强制）。
    pub inviter: NodeAddr,
    pub expires_at: u64,
    pub transport_policy: TransportPolicy,
    /// 仅供确认界面展示，不参与授权决策。
    pub display_name: String,
    /// 仅供确认界面展示，不参与授权决策。
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
    capability: [u8; 16],
    /// NodeId 的 multihash 字节（ed25519 下 38B；验签公钥由此恢复）。
    inviter_id: Vec<u8>,
    /// multiaddr 二进制（文本形态约 2x 膨胀，QR 长度敏感）。
    inviter_addrs: Vec<Vec<u8>>,
    expires_at: u64,
    /// 0 = Auto，1 = LocalOnly。
    transport_policy: u8,
    display_name: String,
    display_platform: String,
    /// 必须末位（postcard 定长数组无长度前缀 → wire 尾部恰为 64 字节裸签名）。
    /// serde 内置 impl 只到 [u8;32]，64 字节拆两段序列化——postcard 下仍是紧凑
    /// 64 字节无分隔，尾部恰为签名（切分契约不受影响）。
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
    /// 前缀不是 `sd`（不是邀请串或种类不对）。
    #[error("不是配对邀请串（缺 {KIND} 前缀）")]
    Kind,
    /// Base64URL / Base32 外层文本解码失败。
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
            capability: rand::RngExt::random(&mut rng),
            inviter: NodeAddr::with_addrs(secret.node_id(), inviter_addrs),
            expires_at: now + INVITE_TTL_SECS,
            transport_policy,
            display_name,
            display_platform,
        }
    }

    /// 编码为链接分享用的 URL-safe Base64 文本并签名。
    ///
    /// 二维码渲染会将同一份已验签 wire 改为 Base32 表现形式；不能直接把 Base64URL
    /// 大写化，否则会破坏其大小写敏感的 payload。
    pub fn encode(&self, secret: &SecretKey) -> String {
        let mut wire = self.to_wire([0u8; 64]);
        // 签名尾置：先序列化占位版取 signable（尾 64 字节即占位签名，前缀与最终
        // 序列化逐字节一致），签完写回再序列化——覆盖含 enum 判别码在内的全部前置字节。
        let unsigned = postcard::to_stdvec(&InviteWire::V1(wire.clone())).expect("postcard");
        let sig = secret.sign(&unsigned[..unsigned.len() - 64]);
        wire.signature = sig;
        let bytes = postcard::to_stdvec(&InviteWire::V1(wire)).expect("postcard");
        format!("{KIND}:{}", BASE64URL_NOPAD.encode(&bytes))
    }

    /// 解码并**验签**链接或二维码邀请。
    ///
    /// 链接形态是大小写敏感的 Base64URL；二维码形态是大小写不敏感的 Base32。TTL
    /// 由调用方按 `expires_at` 判定——权威判定在发起端 [`InviteRegistry`]，解码侧预检
    /// 仅为 UX。
    pub fn decode(s: &str) -> Result<Self, InviteParseError> {
        let bytes = decode_wire_text(s)?;
        Self::decode_wire(&bytes)
    }

    fn decode_wire(bytes: &[u8]) -> Result<Self, InviteParseError> {
        if bytes.len() <= 64 {
            return Err(InviteParseError::Verify("载荷过短"));
        }
        let InviteWire::V1(wire) =
            postcard::from_bytes(bytes).map_err(|e| InviteParseError::Postcard(e.to_string()))?;

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
            capability: wire.capability,
            inviter: NodeAddr::with_addrs(inviter_id, addrs),
            expires_at: wire.expires_at,
            transport_policy,
            display_name: wire.display_name,
            display_platform: wire.display_platform,
        })
    }

    /// 将一个已验签的链接/二维码邀请转为二维码专用的 Base32 文本。
    ///
    /// 这不是另一份 wire：只改变外层文本编码，避免 Base64URL 在二维码统一大写化时损坏。
    pub(crate) fn qr_payload(s: &str) -> Result<String, InviteParseError> {
        let bytes = decode_wire_text(s)?;
        Self::decode_wire(&bytes)?;
        Ok(format!("{KIND}{}", BASE32_NOPAD.encode(&bytes)))
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
            capability: self.capability,
            inviter_id: self.inviter.id.to_bytes(),
            inviter_addrs: self.inviter.addrs.iter().map(|a| a.to_bytes()).collect(),
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

/// 解开外层文本表现形式。带 `:` 的链接形态必须保持大小写；无 `:` 的二维码形态才可
/// 大写化，因为它使用的是大小写无关的 Base32。
fn decode_wire_text(s: &str) -> Result<Vec<u8>, InviteParseError> {
    let s = s.trim();
    let rest = s
        .get(..KIND.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(KIND))
        .map(|_| &s[KIND.len()..])
        .ok_or(InviteParseError::Kind)?;

    if let Some(payload) = rest.strip_prefix(':') {
        return BASE64URL_NOPAD
            .decode(payload.as_bytes())
            .map_err(|e| InviteParseError::Encoding(e.to_string()));
    }

    BASE32_NOPAD
        .decode(rest.to_ascii_uppercase().as_bytes())
        .map_err(|e| InviteParseError::Encoding(e.to_string()))
}

/// 解析邀请串（含验签）。注意编码需私钥签名 → 无对称 `Display`（见 [`PairInvite::encode`]）。
impl FromStr for PairInvite {
    type Err = InviteParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(s)
    }
}

/// 邀请消费被拒的原因（发起端权威判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteRejectReason {
    /// 未知 capability（从未发出、凭据错误或发起端已重启）。
    Unknown,
    /// 已过期。
    Expired,
    /// 已被消费（一次性）或已撤销。
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InviteState {
    Pending,
    Consumed,
    Revoked,
}

struct PendingInvite {
    expires_at: u64,
    state: InviteState,
}

impl PendingInvite {
    fn check_available(&self, now: u64) -> Result<(), InviteRejectReason> {
        if now >= self.expires_at {
            return Err(InviteRejectReason::Expired);
        }
        if self.state != InviteState::Pending {
            return Err(InviteRejectReason::Unavailable);
        }
        Ok(())
    }
}

/// 发起端邀请状态表：TTL + 哈希校验 + **原子一次性消费**（内存态）。
#[derive(Default)]
pub struct InviteRegistry {
    invites: Mutex<HashMap<[u8; 32], PendingInvite>>,
}

impl InviteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记新生成的邀请（以 capability 哈希为键，明文不进本表/日志/持久化）。
    pub fn register(&self, invite: &PairInvite) {
        self.invites.lock().expect("registry lock").insert(
            capability_hash(&invite.capability),
            PendingInvite {
                expires_at: invite.expires_at,
                state: InviteState::Pending,
            },
        );
    }

    /// **非消费**预检（入站请求到达时早拒明显非法，不占用一次性额度）：
    /// 存在 + 未过期 + capability 哈希匹配 + 状态 Pending。权威消费仍在
    /// [`try_consume`](Self::try_consume)（用户确认时）。
    pub fn check(&self, capability: &[u8; 16], now: u64) -> Result<(), InviteRejectReason> {
        let invites = self.invites.lock().expect("registry lock");
        let entry = invites
            .get(&capability_hash(capability))
            .ok_or(InviteRejectReason::Unknown)?;
        entry.check_available(now)
    }

    /// 用户确认时调用：TTL + capability 哈希 + CAS `Pending → Consumed`。
    ///
    /// 一次性语义靠单锁内的检查-置换完成——两台设备同时扫同一码时恰有一台成功
    /// （另一台拿到 [`InviteRejectReason::Unavailable`]）。
    pub fn try_consume(&self, capability: &[u8; 16], now: u64) -> Result<(), InviteRejectReason> {
        let mut invites = self.invites.lock().expect("registry lock");
        let entry = invites
            .get_mut(&capability_hash(capability))
            .ok_or(InviteRejectReason::Unknown)?;
        entry.check_available(now)?;
        entry.state = InviteState::Consumed;
        Ok(())
    }

    /// 撤销（用户取消 / 界面关闭）。
    pub fn revoke(&self, capability: &[u8; 16]) {
        if let Some(e) = self
            .invites
            .lock()
            .expect("registry lock")
            .get_mut(&capability_hash(capability))
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

/// capability 只作为一次性 bearer 凭据使用；状态表以其哈希索引，避免明文滞留。
fn capability_hash(capability: &[u8; 16]) -> [u8; 32] {
    Sha256::digest(capability).into()
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
    fn roundtrip_url_and_qr_text() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let s = invite.encode(&sk);
        assert!(s.starts_with(&format!("{KIND}:")));
        assert!(!s.ends_with('='), "URL 编码不应带 padding");
        assert!(s.len() < PairInvite::qr_payload(&s).unwrap().len());
        // 链接原样解码
        let back = PairInvite::decode(&s).unwrap();
        assert_eq!(back, invite);
        assert!(PairInvite::decode(&s.to_ascii_uppercase()).is_err());
        // 二维码形态可被整串大写，Base64URL 链接则不能被大写化。
        let qr = PairInvite::qr_payload(&s).unwrap();
        assert_eq!(PairInvite::decode(&qr).unwrap(), invite);
        assert_eq!(
            PairInvite::decode(&qr.to_ascii_uppercase()).unwrap(),
            invite
        );
        // 二维码前缀大小写混排也应放行。
        assert_eq!(
            PairInvite::decode(&format!("Sd{}", &qr[KIND.len()..])).unwrap(),
            invite
        );
    }

    #[test]
    fn tampered_fields_are_rejected() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::LocalOnly);
        let s = invite.encode(&sk);
        let rest = s.strip_prefix(&format!("{KIND}:")).unwrap();
        let bytes = BASE64URL_NOPAD.decode(rest.as_bytes()).unwrap();

        // 逐字节翻转除签名外的每个字节（含 enum 判别码与 transport_policy），必须全拒
        for i in 0..bytes.len() - 64 {
            let mut tampered = bytes.clone();
            tampered[i] ^= 0x01;
            let ts = format!("{KIND}:{}", BASE64URL_NOPAD.encode(&tampered));
            assert!(
                PairInvite::decode(&ts).is_err(),
                "第 {i} 字节被篡改却通过了解码"
            );
        }
        // 篡改签名本身也拒
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let ts = format!("{KIND}:{}", BASE64URL_NOPAD.encode(&tampered));
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
            PairInvite::decode("sd:!!!!"),
            Err(InviteParseError::Encoding(_))
        ));
        // 前缀对、Base64URL 合法、内容不是 postcard wire
        let junk = format!("{KIND}:{}", BASE64URL_NOPAD.encode(&[9u8; 80]));
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
        // 错 capability
        assert_eq!(
            reg.try_consume(&[0u8; 16], invite.expires_at - INVITE_TTL_SECS),
            Err(InviteRejectReason::Unknown)
        );
        // 过期
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at),
            Err(InviteRejectReason::Expired)
        );
        // 正常消费
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS),
            Ok(())
        );
        // 重复消费拒
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS),
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
            let cap = invite.capability;
            let now = invite.expires_at - INVITE_TTL_SECS;
            handles.push(std::thread::spawn(move || {
                reg.try_consume(&cap, now).is_ok()
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
        reg.revoke(&invite.capability);
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS),
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// 撤销侧（三端 UI）手上只有邀请串，capability 要经解码取回。这条钉死
    /// 「编解码往返后仍索引到同一条记录」——`PairingManager::revoke_invite` 的不变量。
    #[test]
    fn revoke_via_decoded_invite_string_blocks_consume() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = InviteRegistry::new();
        reg.register(&invite);

        let decoded = PairInvite::decode(&invite.encode(&sk)).unwrap();
        reg.revoke(&decoded.capability);

        assert_eq!(
            reg.check(&invite.capability, invite.expires_at - INVITE_TTL_SECS),
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// wire 契约锁定：V1 的关键字段布局。**本测试失败 = wire 契约被改动**——
    /// 已发布的邀请串将无法解析，禁止随手"修"这个测试，先回看 InviteV1 的改动。
    #[test]
    fn wire_v1_keeps_version_capability_and_tail_signature_layout() {
        let sk = SecretKey::generate();
        let mut invite = test_invite(&sk, TransportPolicy::LocalOnly);
        invite.capability = [0x22; 16];
        let s = invite.encode(&sk);
        let bytes = decode_wire_text(&s).unwrap();
        // 契约固定段（不含随机密钥派生部分）：
        // [0]=0x00 enum 判别码（V1）；[1..17]=capability
        assert_eq!(bytes[0], 0x00, "V1 判别码必须是 0x00");
        assert_eq!(&bytes[1..17], &[0x22; 16]);
        // 尾 64 字节是签名（签名尾置契约）
        let sig: [u8; 64] = bytes[bytes.len() - 64..].try_into().unwrap();
        assert!(invite.inviter.id.verify(&bytes[..bytes.len() - 64], &sig));
    }
}
