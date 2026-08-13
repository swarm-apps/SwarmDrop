//! PairInvite：一次性签名配对邀请（openspec: pair-invite-protocol）。
//!
//! 替代 6 位配对码的信任建立机制：发起方生成自包含邀请串（Ed25519 签名 + 128bit
//! capability + TTL + 一次性消费），二维码/链接是同一字符串的不同载体。
//!
//! 编码骨架借自 iroh-tickets（源码级调研 2026-07-19，设计记录见
//! `openspec/changes/pair-invite-protocol/design.md`）：
//! - **唯一文本形态** = canonical URL（[`INVITE_URL_PREFIX`]）+ base32-nopad payload。
//!   链接分享、二维码、剪贴板感知、深链 payload 四种用法共用同一个字符串
//!   （openspec: invite-url-canonical）。
//! - 二维码内容就是这个串**原样**（不做大小写归一）：payload 是大写 base32，落在 QR
//!   alphanumeric 字符集内；小写的 scheme/host/path 前缀由编码器的最优分段单独成段。
//!   实测两者同一 version，故没必要为二维码牺牲链接外观（见 [`qr`](crate::qr)）。
//! - 选 base32 而非 base64url 的理由：**大小写不敏感**。payload 因此能用大写字母表进
//!   QR alphanumeric，解析时也能容忍任意大小写的输入（手抄、IM 自动首字母大写等）。
//! - wire = postcard 单变体 enum（[`InviteWire`]，1 字节判别码即版本；未知变体解码
//!   即失败）+ 手工镜像结构（领域类型改字段不碰 wire 契约）
//! - **签名对象显式**：`SIGNING_DOMAIN ‖ core 字节`，标签含版本标识（防跨版本复用）。
//!   ⚠️ wire v1 曾靠「`signature` 是末位定长 64 字节 ⇒ signable = `bytes[..len-64]`」这条
//!   **位置约定**取签名对象；v2 把地址提示移出签名覆盖范围后签名不再位于末尾，那条约定
//!   既不成立、也不该被换成另一条同样脆弱的。加字段时不要把它复活
//! - **地址提示不进签名**（见 [`InviteV2`]），因此条数受 [`MAX_ADDR_HINTS`] 限制、
//!   且零地址的邀请一律拒收
//! - 验签公钥从 `inviter_id` 就地恢复（ed25519 PeerId 是 identity multihash），
//!   邀请不携带独立公钥字段
//!
//! 与 iroh ticket 的关键差异（它无签名/TTL/一次性）：签名兜底的是身份 pin 覆盖不到
//! 的字段完整性——首要是 `transport_policy`（LocalOnly 承诺不被中间人降级为 Auto）。
//! capability/TTL/一次性全在发起端 [`InviteRegistry`]。
//!
//! 它**不再是纯内存态**（openspec: invite-persistence）：TTL 放宽到 24h 后经
//! [`InviteStore`] 端口落盘，跨重启存活 —— 否则「发条链接给同事、自己顺手重启 App」
//! 这条最普通的路径必然失败。内存表仍是一次性消费的**权威判定点**，落盘只是写穿备份。
//! 「只存 capability 哈希、明文绝不落盘/日志」这条不变量不因落盘而放松，反而更硬。

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use data_encoding::BASE32_NOPAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swarmdrop_net_base::compact::{self, CompactAddrs};
use swarmdrop_net_base::{Addr, NodeAddr, NodeId, SecretKey};

use crate::store::{InviteRecord, InviteStore, NoopInviteStore, PersistedInviteState};

/// **生成**邀请链接用的 canonical 前缀（唯一主前缀）。
///
/// 当前指 GitHub Pages 子路径：`swarmapp.cn` 已实名但**尚未备案**，境内注册商可以随时把
/// 未备案 `.cn` 的解析停掉 —— 拿它当邀请链接的载体等于把配对功能挂在一个可被第三方
/// 关停的开关上。备案下来后把这里换成 `https://swarmapp.cn/p/#`，
/// **同时把旧前缀留在 [`ACCEPTED_URL_PREFIXES`] 里**（见那里的理由）。
///
/// 换前缀要同步的地方（跨语言，编译器管不到）：
///
/// | 位置 | 性质 |
/// |---|---|
/// | `.github/workflows/docs.yml` 的 `PAGES_BASE_PATH` / `PAGES_SITE_ORIGIN` | 站点得真的部署在这个地址上 |
/// | `mobile/src/core/invite-link.ts` 的正则 | **功能性**，但已含两个域名的 alternation，切换无需再改 |
/// | `docs/app/app/_lib/invite.ts` 的 `INVITE_URL_PREFIX` / `INVITE_LINK_SOURCE` | **功能性**，Web 应用区唯一的一份副本 |
/// | `docs/public/p/index.html` 的 `/p/` 路径段 | **功能性**，但域名自适应（见下） |
/// | `src/routes/_app/pairing/input.lazy.tsx` 的 placeholder | 纯文案 |
/// | `docs/content/docs/*.mdx`、README | 纯文档 |
///
/// Web 端两条路径的性质**不一样**，别当成一件事：
/// - **落地页**（`docs/public/p/index.html`）从 `location` 推导前缀，所以换域名它自适应；
///   但它硬编码了 `/p/` 这个**路径段**——路径一改它会静默拼出不受理的前缀，
///   症状是「落地页显示成功、应用区说这不是配对邀请」。
/// - **应用区**（`_lib/invite.ts`）硬编码整个前缀，不能用 `location.origin`：解码器只认
///   受理列表里的前缀，而站点跑在预览域名或 localhost 时推导出来的串会被拒掉。
///
/// 两处约束，改动前先读明白：
/// 1. **带尾斜杠**。静态导出产物是 `p/index.html`，访问 `/p` 会先吃一次目录重定向；
///    带上 `/` 省掉那一跳，也避开重定向丢 fragment 的风险。
/// 2. **capability 在 fragment（`#`）不在 path/query**。`#` 之后的内容浏览器不发给
///    服务器，边缘日志与 Referer 都拿不到这个 128bit 信任凭证。
///
/// 前缀全小写、payload 大写 base32 —— 这是常规 URL 外观。**不要为了二维码把它改成整串
/// 大写**：那是 fast_qr 时代（全串统一 mode）的遗留约束，换成支持最优分段的编码器后
/// 大写对码面零收益，`qr::tests::uppercasing_does_not_shrink_the_code` 钉着这一点。
const INVITE_URL_PREFIX: &str = "https://swarm-apps.github.io/SwarmDrop/p/#";

/// **解码**时受理的前缀集合，含 [`INVITE_URL_PREFIX`] 与所有历史/未来前缀。
///
/// 为什么不是只认主前缀：域名迁移会有一段两种链接同时在外面飘的时期（邀请活 24h，而
/// 用户手上的 App 版本也不会同一秒更新）。只认主前缀 = 迁移当天所有在途邀请一起失效，
/// 而症状是「链接看着好好的，点开说不是配对邀请」，最难自查。
///
/// 受理多个前缀**不放松任何安全性**：前缀从来不是信任边界，签名才是（`evil.example`
/// 那条负例仍然拒 —— 它不在本列表里）。前缀只决定「这段文本里的 payload 从哪开始」。
const ACCEPTED_URL_PREFIXES: &[&str] = &[
    INVITE_URL_PREFIX,
    // 备案完成后会成为主前缀；提前受理，届时切换不会打断在途邀请。
    "https://swarmapp.cn/p/#",
];

/// 默认 TTL：24 小时。
///
/// 曾经是 5 分钟 —— 那对「当面扫码」恰当，对「发条链接给同事」是灾难：对方十分钟后点开、
/// 或者自己顺手重启了 App，邀请就没了。载体变成可分享的链接（openspec:
/// invite-url-canonical）之后，异步远程才是主场景，所以放宽到 24h 并让注册表落盘
/// （openspec: invite-persistence）。
///
/// 真正的保护从来不是 TTL，而是一次性消费 + 验签 + 可撤销 + 只存 capability 哈希；
/// TTL 只是兜底。
pub const INVITE_TTL_SECS: u64 = 86_400;

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

/// wire 层：postcard 单变体 enum（判别码即版本）。
///
/// **只认当前版本，不带旧版本解码分支。** 邀请是 TTL 24 小时的一次性凭证，跨版本共存窗口
/// 自然收敛；保留旧解码换来的是一个只在极窄时间窗被执行到、因而没人测过的分支。旧邀请的
/// 失败形态是「不是有效的配对邀请」，用户请对方重新生成一条即可，可自恢复。
#[derive(Serialize, Deserialize)]
enum InviteWire {
    V2(InviteV2),
}

/// 三段结构：**受保护区**（`core`）、签名、**不受保护的地址提示**（`hints`）。
///
/// # 为什么 `core` 是一段 `Vec<u8>` 而不是内联的结构体
///
/// 签名对象必须是一段**明确的字节**。V1 的做法是「序列化一遍占位签名的整条 wire，切掉末
/// 64 字节当 signable」，于是背着一条隐式契约：`signature` 必须是最后一个字段。地址提示
/// 移出签名覆盖范围后签名不再位于末尾，那条契约既不成立、也不该换成另一条同样脆弱的位置
/// 约定。把 `SignedCore` 单独序列化成一段字节，签名对象就是它本身，代价是一个长度前缀。
///
/// # 为什么 `hints` 不进签名
///
/// 地址提示是**下游自证**的：拨过去之后身份由传输层（Noise / QUIC-TLS）对已签名的
/// `inviter_id` 强制校验，篡改地址只能导致拨号失败，或把受邀方引向一个**完不成握手**的
/// 第三方。签名保护的是 capability 的真实性与 `LocalOnly` 承诺不可降级 —— 两样都在
/// `core` 里。
///
/// 代价如实记：能改写邀请文本的攻击者可以删空或替换地址提示，使受邀方拨不通。那是拒绝
/// 服务，与「把整串改坏」同级，不涉及身份或凭证。
///
/// 换来的是[`SignedInvite`]：签一次之后地址可以任意增删而无需私钥 —— 「按二维码密度回收
/// 地址」于是从生成侧的一次性动作变成任何持串方都能做的纯结构操作。
#[derive(Serialize, Deserialize)]
struct InviteV2 {
    /// `postcard(SignedCore)` 的字节。签名对象是 [`SIGNING_DOMAIN`] 与它的拼接。
    core: Vec<u8>,
    /// serde 内置数组 impl 上限 32，拆两段序列化；postcard 下仍是紧凑 64 字节无分隔。
    #[serde(with = "sig_serde")]
    signature: [u8; 64],
    /// 地址提示。**不在签名覆盖范围内**，可被任意方裁剪。
    hints: CompactAddrs,
}

/// 受签名保护的字段。字段序即契约，发布后不可改动顺序/类型。
#[derive(Serialize, Deserialize)]
struct SignedCore {
    capability: [u8; 16],
    /// NodeId 的 multihash 字节（ed25519 下 38B；验签公钥由此恢复）。
    inviter_id: Vec<u8>,
    expires_at: u64,
    /// 0 = Auto，1 = LocalOnly。
    transport_policy: u8,
    display_name: String,
    display_platform: String,
}

/// 一条邀请最多携带多少条地址提示。
///
/// **这是安全边界，不是容量优化。** 地址提示不在签名覆盖范围内（见 [`InviteV2`]），于是
/// 任何能改写邀请文本的人都能往里塞任意多条。而受邀方拿到它们之后会经
/// `Endpoint::add_addrs` 全部登记并拨号 —— 也就是说不设上限，一条被改写的邀请就是一台
/// 指哪打哪的连接洪泛发生器（`LocalOnly` 也拦不住：它过滤成 `is_private_lan()`，
/// 那恰好是一份内网端口扫描清单）。
///
/// wire v1 的签名覆盖地址列表，所以这个口子是 v2 新开的，必须在 v2 里堵上。
///
/// 32 是从生成侧反推的：`select_invite_addrs` 最多 5 个网络类别 × 每类 3 条传输 = 15。
/// 留一倍余量给将来新增的传输或类别，超出即判为异常输入。
const MAX_ADDR_HINTS: usize = 32;

/// 签名的域分隔标签，含版本标识。
///
/// 它让**跨版本的签名复用**天然失败：同样一组字段，V2 的签名搬到别的版本上验不过。
/// V1 靠「签名覆盖含 enum 判别码在内的全部前置字节」达到同一效果，但那是位置约定的副产品；
/// 这里是显式的。
const SIGNING_DOMAIN: &[u8] = b"swarmdrop-invite-v2";

/// 待签名的字节：域分隔标签 ‖ `core`。
fn signing_message(core: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SIGNING_DOMAIN.len() + core.len());
    message.extend_from_slice(SIGNING_DOMAIN);
    message.extend_from_slice(core);
    message
}

/// 从任意文本里定位邀请并解出 wire 字节。
fn decode_payload_bytes(s: &str) -> Result<Vec<u8>, InviteParseError> {
    BASE32_NOPAD
        .decode(extract_payload(s)?.to_ascii_uppercase().as_bytes())
        .map_err(|e| InviteParseError::Encoding(e.to_string()))
}

/// 解 wire 并**验签**。[`PairInvite::decode`] 与 [`SignedInvite::decode`] 的共同前半段——
/// 两个出口共用同一段校验，不存在「某个入口松一点」的可能。
fn verify_wire(bytes: &[u8]) -> Result<(InviteV2, SignedCore, NodeId), InviteParseError> {
    let InviteWire::V2(wire) =
        postcard::from_bytes(bytes).map_err(|e| InviteParseError::Postcard(e.to_string()))?;
    let core: SignedCore =
        postcard::from_bytes(&wire.core).map_err(|e| InviteParseError::Postcard(e.to_string()))?;
    let inviter_id = NodeId::from_bytes(&core.inviter_id)
        .map_err(|_| InviteParseError::Verify("发起方身份非法"))?;
    if !inviter_id.verify(&signing_message(&wire.core), &wire.signature) {
        return Err(InviteParseError::Verify("签名无效（邀请被篡改或伪造）"));
    }
    Ok((wire, core, inviter_id))
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

/// 本机当前没有任何可拨地址，因此**造不出**一条有意义的邀请。
///
/// 这是「一条邀请至少携带一条可拨地址」这条不变量的**生成侧**一半，与
/// [`PairInvite::decode`] 里那句「邀请没有任何可拨地址」是同一条判断的两端：
/// 那边保证本机不接受这种邀请，这边保证本机不产出它。
///
/// **为什么必须在这里挡住，而不是编出来再说。** 零地址邀请编得出、扫得动、也复制得走，
/// 唯独没有任何东西可拨。此前生成侧不挡，于是发起方看到一张完全正常的二维码，受邀方拿到
/// 它才报错 —— 认知分叉在两台设备之间，发起方永远不知道自己发出去的是废码。最容易撞上的
/// 是浏览器端：它不 listen 本地 socket，可拨地址全部来自 relay reservation，在
/// reservation 落定之前生成的每一条邀请都是空的。
///
/// 它是**瞬态**的，不是配置错误：等网络地址就绪后重新生成即可。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("本机当前没有任何可拨地址，邀请至少要携带一条")]
pub struct NoDialableAddrs;

/// 邀请串解析错误（分类照 iroh-tickets 的 ParseError 四分层）。
#[derive(Debug, thiserror::Error)]
pub enum InviteParseError {
    /// 文本里找不到 canonical 前缀（不是邀请链接，或域名不对）。
    #[error("不是配对邀请链接（文本中未找到 {INVITE_URL_PREFIX}）")]
    Kind,
    /// base32 外层文本解码失败（前缀对但 payload 损坏 / 被截断）。
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
    /// 生成一个新邀请。`now` 为 Unix 秒（时间源由调用方注入，便于测试与 wasm）。
    ///
    /// `inviter_addrs` 为空即 [`NoDialableAddrs`] —— 见那里的推导。这条闸让**「地址恒非空」
    /// 成为 `PairInvite` 的类型不变量**：它只有两条构造路径（本方法与 [`Self::decode`]），
    /// 两条都守住了，于是「拿到一个 `PairInvite` 就一定有地方可拨」在类型层面成立，下游
    /// 不必各自再判一次空。
    pub fn generate(
        secret: &SecretKey,
        inviter_addrs: Vec<Addr>,
        transport_policy: TransportPolicy,
        display_name: String,
        display_platform: String,
        now: u64,
    ) -> Result<Self, NoDialableAddrs> {
        if inviter_addrs.is_empty() {
            return Err(NoDialableAddrs);
        }
        let mut rng = rand::rng();
        Ok(Self {
            capability: rand::RngExt::random(&mut rng),
            inviter: NodeAddr::with_addrs(secret.node_id(), inviter_addrs),
            expires_at: now + INVITE_TTL_SECS,
            transport_policy,
            display_name,
            display_platform,
        })
    }

    /// 签名并编码为 canonical 邀请链接（[`INVITE_URL_PREFIX`] + base32-nopad）。
    ///
    /// 产物直接用于链接分享、剪贴板、深链**与二维码** —— 四种载体同一个字符串，
    /// 二维码不再需要另一种编码、也不做大小写归一（见模块文档与 [`qr`](crate::qr)）。
    pub fn encode(&self, secret: &SecretKey) -> String {
        self.sign(secret).encode()
    }

    /// 签名一次，得到一份**地址可增删**的邀请。
    ///
    /// 这是「裁剪不需要私钥」这条性质的入口：[`SignedInvite::encode`] 不吃密钥，所以
    /// 「一边裁地址一边重编码」的循环在类型上就不可能顺带重签名。
    pub fn sign(&self, secret: &SecretKey) -> SignedInvite {
        let core = postcard::to_stdvec(&SignedCore {
            capability: self.capability,
            inviter_id: self.inviter.id.to_bytes(),
            expires_at: self.expires_at,
            transport_policy: match self.transport_policy {
                TransportPolicy::Auto => 0,
                TransportPolicy::LocalOnly => 1,
            },
            display_name: self.display_name.clone(),
            display_platform: self.display_platform.clone(),
        })
        .expect("postcard");
        let signature = secret.sign(&signing_message(&core));
        SignedInvite {
            core,
            signature,
            addrs: self.inviter.addrs.clone(),
        }
    }

    /// 从任意文本中提取邀请并**验签** —— 三端唯一的解析入口。
    ///
    /// 输入不要求是干净的邀请串：会在文本中定位 canonical 前缀（大小写不敏感），
    /// 再吃掉紧随其后的 base32 字符。于是「IM 里连着说明文字一起复制」「前后带空白」
    /// 「被某个客户端整串大写化」都能认，调用方无需先清洗。
    ///
    /// TTL 由调用方按 `expires_at` 判定 —— 权威判定在发起端 [`InviteRegistry`]，
    /// 解码侧预检仅为 UX。
    pub fn decode(s: &str) -> Result<Self, InviteParseError> {
        Self::decode_wire(&decode_payload_bytes(s)?)
    }

    fn decode_wire(bytes: &[u8]) -> Result<Self, InviteParseError> {
        let (wire, core, inviter_id) = verify_wire(bytes)?;

        let transport_policy = match core.transport_policy {
            0 => TransportPolicy::Auto,
            1 => TransportPolicy::LocalOnly,
            _ => return Err(InviteParseError::Verify("未知网络策略")),
        };
        // 地址提示不受签名保护 ⇒ 条数是攻击者可控的，必须设上限。见 [`MAX_ADDR_HINTS`]。
        if wire.hints.len() > MAX_ADDR_HINTS {
            return Err(InviteParseError::Verify("地址提示数量异常"));
        }
        // 单条还原不出来时 `unpack` 跳过它继续（见 `swarmdrop_net_base::compact`）——少一条
        // 只是少一条可拨路径。但**一条都不剩**是另一回事。
        //
        // ⚠️ 判据是「零地址就拒」，**不是**「有提示但全丢了才拒」。后者是本改动第一版写的，
        // 漏掉了最容易构造的那种篡改：把路径数直接改成 0。两者的结果一模一样 —— 一条
        // 验签通过、字段完好、就是没有任何东西可拨的邀请，用户要等到 RPC 超时才看到一句
        // 与病因无关的「发送配对请求失败」。
        //
        // 这与 `swarmdrop_invite::compose` 里「裁剪绝不裁到零」是同一条价值判断的两端：
        // 那边保证本机不产出这种邀请，这边保证本机不接受它。
        let addrs = compact::unpack(&wire.hints);
        if addrs.is_empty() {
            return Err(InviteParseError::Verify("邀请没有任何可拨地址"));
        }
        Ok(Self {
            capability: core.capability,
            inviter: NodeAddr::with_addrs(inviter_id, addrs),
            expires_at: core.expires_at,
            transport_policy,
            display_name: core.display_name,
            display_platform: core.display_platform,
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
}

/// 一份已签名的邀请：受保护区与签名已固定，**地址提示仍可增删**。
///
/// 存在的理由是让「按二维码密度回收地址」这件事不再需要私钥。[`encode`](Self::encode) 不吃
/// 密钥，所以调用方即使写一个「编码 → 太密 → 丢一条 → 再编码」的循环，也不可能顺带把
/// 签名重算一遍 —— 那条性质由类型保证，不靠注释提醒。
///
/// 地址提示不受签名保护（理由见 [`InviteV2`]），所以增删是**结构操作**而非伪造：
/// 产物仍能通过验签，capability / 身份 / TTL / 策略一字不变。
pub struct SignedInvite {
    core: Vec<u8>,
    signature: [u8; 64],
    addrs: Vec<Addr>,
}

impl SignedInvite {
    /// 从邀请串还原，**保留签名**。
    ///
    /// 与 [`PairInvite::decode`] 的差别只有一条，但那条决定了用途：那边把签名丢掉（它只回答
    /// 「这条邀请说了什么」），于是重编码不回去；这边留着，所以可以裁掉几条地址再编码成一条
    /// 仍然有效的邀请 —— 二维码按自己的码面回收地址就靠它。
    ///
    /// 校验与 `PairInvite::decode` 完全同款（同一段 `decode_wire` 逻辑的两个出口），
    /// **不放松任何一步**。
    pub fn decode(s: &str) -> Result<Self, InviteParseError> {
        let bytes = decode_payload_bytes(s)?;
        let (wire, _core, _id) = verify_wire(&bytes)?;
        Ok(Self {
            addrs: compact::unpack(&wire.hints),
            core: wire.core,
            signature: wire.signature,
        })
    }

    /// 编码为 canonical 邀请链接。**不需要私钥**。
    pub fn encode(&self) -> String {
        let bytes = postcard::to_stdvec(&InviteWire::V2(InviteV2 {
            core: self.core.clone(),
            signature: self.signature,
            hints: compact::pack(&self.addrs),
        }))
        .expect("postcard");
        format!("{INVITE_URL_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
    }

    /// 当前携带的地址提示。
    pub fn addrs(&self) -> &[Addr] {
        &self.addrs
    }

    /// 可增删的地址提示。
    ///
    /// 开放**写**入口而不是只给一个 `remove`：提示不受签名保护，增与删在密码学上是同一件事，
    /// 假装只能删只会让调用方绕路。真正的约束在别处，而且是两端各守一半：产出侧由
    /// [`compose::drop_least_valuable_addr`](crate::compose) 保证裁剪绝不裁到零，
    /// 接收侧由 [`PairInvite::decode`] 拒掉零地址邀请。
    pub fn addrs_mut(&mut self) -> &mut Vec<Addr> {
        &mut self.addrs
    }
}

/// 在任意文本里定位 canonical 邀请并切出其 base32 payload。
///
/// 前缀匹配大小写不敏感（防外部客户端把链接大写化 —— 二维码本身已不再大写，见 `qr`），
/// payload 取「紧随前缀的连续 base32 字符」——于是尾随的标点、空白、换行、后续文字
/// 都自然被排除。
fn extract_payload(s: &str) -> Result<&str, InviteParseError> {
    // ASCII 大小写转换是逐字节等长的，非 ASCII 字节（比如 IM 里一起复制过来的中文）
    // 原样保留，故 lowered 上的字节下标可以直接用于切原串。
    let lowered = s.to_ascii_lowercase();
    // **前缀也要小写化再比**，不能拿常量原样去 find：GitHub Pages 的路径段是仓库名
    // `SwarmDrop`（带大写），而 haystack 已经小写化了 —— 直接比会永远匹配不上，
    // 且症状是「不是配对邀请链接」，看不出是大小写问题。
    // 生成侧必须保留原大小写（Pages 路径区分大小写，`/swarmdrop/` 是 404），
    // 所以「生成用精确大小写、匹配不敏感」这个不对称是刻意的。
    //
    // 取**最靠前**命中的那个前缀。多个前缀同时出现只会在人为拼接的文本里发生，
    // 取最前与「从头扫一遍找第一条邀请」的直觉一致。
    let (start, prefix_len) = ACCEPTED_URL_PREFIXES
        .iter()
        .filter_map(|p| {
            lowered
                .find(&p.to_ascii_lowercase())
                .map(|at| (at, p.len()))
        })
        .min_by_key(|(at, _)| *at)
        .ok_or(InviteParseError::Kind)?;
    let payload_start = start + prefix_len;
    let payload_len = s[payload_start..]
        .bytes()
        .take_while(|b| is_base32_char(*b))
        .count();
    if payload_len == 0 {
        return Err(InviteParseError::Encoding("链接缺少 payload".into()));
    }
    Ok(&s[payload_start..payload_start + payload_len])
}

/// RFC 4648 base32 字母表（`A-Z2-7`），大小写均放行。
fn is_base32_char(b: u8) -> bool {
    b.is_ascii_alphabetic() || (b'2'..=b'7').contains(&b)
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
    /// 消费已在内存里生效，但**没能落盘** —— 见 [`InviteRegistry::try_consume`]。
    ///
    /// 与其它三个不同，它不是「这份邀请有问题」，而是「本机没法保证一次性语义」，
    /// 所以宁可让这次配对诚实地失败（此刻还没告诉对端成功）。
    NotPersisted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InviteState {
    Pending,
    Consumed,
    Revoked,
}

struct PendingInvite {
    /// 落盘记录需要它（发起方身份）；当前恒为本机，留字段是为了不必回头改结构。
    inviter_id: NodeId,
    /// 落盘记录需要它（邀请列表按创建时间倒序）。
    created_at: u64,
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

/// 发起端邀请状态表：TTL + 哈希校验 + **原子一次性消费**。
///
/// 内存表是**权威判定点**，[`InviteStore`] 只是它的写穿备份 —— 一次性语义靠单锁内的
/// 检查-置换完成，不能退化成「先查库再写库」的两段式（那会开出并发双花的窗口）。
/// 每个变更方法都是「锁内改内存 → 释放锁 → await 写穿」，锁不跨 await。
///
/// 落盘让邀请跨重启存活（openspec: invite-persistence）。不注入 store 时用
/// [`NoopInviteStore`] 退回旧的纯内存语义。
pub struct InviteRegistry {
    invites: Mutex<HashMap<[u8; 32], PendingInvite>>,
    store: Arc<dyn InviteStore>,
}

impl Default for InviteRegistry {
    fn default() -> Self {
        Self::new(Arc::new(NoopInviteStore))
    }
}

impl InviteRegistry {
    pub fn new(store: Arc<dyn InviteStore>) -> Self {
        Self {
            invites: Mutex::new(HashMap::new()),
            store,
        }
    }

    /// 从落盘记录恢复内存表（启动时调一次）。
    ///
    /// 顺带清掉已过期的（内存与库一起），避免表随时间无限增长。
    ///
    /// **状态单调**：已在内存里推进到终态的条目不会被库里的 `Pending` 盖回去。
    /// 正常路径下不会撞到（只在全新注册表上调一次），但那是个未被强制的约定 ——
    /// 而写穿失败恰好会造成「内存 Consumed / 库 Pending」，此时重复 load 就让
    /// 已消费的邀请复活了。代价只是一次状态比较，见
    /// `load_never_downgrades_memory_state`。
    pub async fn load(&self, now: u64) {
        let records = self.store.load_all().await;
        {
            let mut invites = self.invites.lock().expect("registry lock");
            for record in records {
                if now >= record.expires_at {
                    continue; // 过期的不进内存表，下面统一从库里删
                }
                if let Some(existing) = invites.get(&record.capability_hash)
                    && existing.state != InviteState::Pending
                {
                    continue; // 内存已是终态，库里的记录不得降级它
                }
                invites.insert(
                    record.capability_hash,
                    PendingInvite {
                        inviter_id: record.inviter_id,
                        created_at: record.created_at,
                        expires_at: record.expires_at,
                        state: match record.state {
                            PersistedInviteState::Pending => InviteState::Pending,
                            PersistedInviteState::Consumed => InviteState::Consumed,
                            PersistedInviteState::Revoked => InviteState::Revoked,
                        },
                    },
                );
            }
        }
        self.store.prune_expired(now).await;
    }

    /// 列出未过期的邀请（供发起方的「已发出邀请」列表；含已消费的，用于显示状态）。
    pub fn list_active(&self, now: u64) -> Vec<InviteSummary> {
        let invites = self.invites.lock().expect("registry lock");
        let mut list: Vec<InviteSummary> = invites
            .iter()
            .filter(|(_, entry)| now < entry.expires_at && entry.state != InviteState::Revoked)
            .map(|(hash, entry)| InviteSummary {
                capability_hash: *hash,
                created_at: entry.created_at,
                expires_at: entry.expires_at,
                consumed: entry.state == InviteState::Consumed,
            })
            .collect();
        // 最近生成的排最前（`Reverse` 而非 `b.cmp(a)`：后者会被 clippy 判为
        // unnecessary_sort_by，而这里排序键是 Copy 的整数，用 sort_by_key 更直接）
        list.sort_by_key(|item| std::cmp::Reverse(item.created_at));
        list
    }

    /// 登记新生成的邀请（以 capability 哈希为键，明文不进本表/日志/持久化）。
    ///
    /// `now` 用作 `created_at`（时间源由调用方注入，与 [`PairInvite::generate`] 一致）。
    pub async fn register(&self, invite: &PairInvite, now: u64) {
        let hash = capability_hash(&invite.capability);
        let record = {
            let mut invites = self.invites.lock().expect("registry lock");
            invites.insert(
                hash,
                PendingInvite {
                    inviter_id: invite.inviter.id,
                    created_at: now,
                    expires_at: invite.expires_at,
                    state: InviteState::Pending,
                },
            );
            InviteRecord {
                capability_hash: hash,
                inviter_id: invite.inviter.id,
                expires_at: invite.expires_at,
                state: PersistedInviteState::Pending,
                created_at: now,
            }
        };
        // 写穿失败在这条路径上是 benign: 库里没有这行 -> 重启后「不认识」-> 拒绝
        // (fail-closed)。唯一后果是用户得重新生成一条，所以这里刻意吞掉返回值。
        let _persisted = self.store.upsert(record).await;
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
    /// （另一台拿到 [`InviteRejectReason::Unavailable`]）。**写穿在锁释放之后**，
    /// 内存态不回滚（回滚会让这份邀请当下就能被再消费一次）。
    ///
    /// **写穿失败返回 [`InviteRejectReason::NotPersisted`]，这条路径必须 fail-closed。**
    /// 否则：库里仍是 `register` 写下的 `Pending`，重启 `load` 把它放回内存 → 同一份
    /// 一次性凭证被消费第二次。而且重启后列表会把它显示成「等待对方使用」——
    /// 用户看不到任何被用过的痕迹，也就不会想到去撤销，唯一的补救信号恰好在这个失败
    /// 模式下消失了。
    ///
    /// 这里能 fail-closed 是因为调用点的顺序：`PairingManager::respond_pairing_request`
    /// 里 `try_consume` 排在 `responder.send(Success)` **之前**，所以此刻配对还没成功，
    /// 报失败是诚实的（代价是极少数情况下用户要重新生成一条邀请）。
    pub async fn try_consume(
        &self,
        capability: &[u8; 16],
        now: u64,
    ) -> Result<(), InviteRejectReason> {
        let hash = capability_hash(capability);
        let record = {
            let mut invites = self.invites.lock().expect("registry lock");
            let entry = invites.get_mut(&hash).ok_or(InviteRejectReason::Unknown)?;
            entry.check_available(now)?;
            entry.state = InviteState::Consumed;
            InviteRecord {
                capability_hash: hash,
                inviter_id: entry.inviter_id,
                expires_at: entry.expires_at,
                state: PersistedInviteState::Consumed,
                created_at: entry.created_at,
            }
        };
        if self.store.upsert(record).await {
            Ok(())
        } else {
            Err(InviteRejectReason::NotPersisted)
        }
    }

    /// 撤销（用户取消 / 界面关闭）。返回是否已落盘，见 [`Self::revoke_by_hash`]。
    pub async fn revoke(&self, capability: &[u8; 16]) -> bool {
        self.revoke_by_hash(capability_hash(capability)).await
    }

    /// 按 capability 哈希撤销 —— 邀请列表（[`Self::list_active`]）手上只有哈希，
    /// 因为明文既不落盘也不出注册表。
    ///
    /// **落盘侧写 `Revoked`，不删行** —— 但这是个 **UX 选择，不是安全改进**：
    /// 让列表能区分「已撤销」与「已被对方使用」，并与 entity 的三态枚举对齐。
    ///
    /// ⚠️ **写穿失败时它仍然 fail-open，写状态和删行在这一点上没有区别**：两种失败都留下
    /// 「库里仍是 `register` 写下的 `Pending`」，重启 `load` 把它放回内存，撤销就失效了。
    /// （早期版本的注释把这条说成安全理由，是错的 —— 由审查用探针实测推翻。）
    ///
    /// 返回**是否已落盘**。撤销是 fire-and-forget 的清理路径，调用方可以忽略；
    /// 但它需要有能力知道「这次撤销没能持久化」，否则用户以为撤销了、重启后邀请却回来了。
    #[must_use = "撤销未落盘时重启后邀请会复活，调用方应当让用户知道"]
    pub async fn revoke_by_hash(&self, capability_hash: [u8; 32]) -> bool {
        let record = {
            let mut invites = self.invites.lock().expect("registry lock");
            match invites.get_mut(&capability_hash) {
                Some(entry) => {
                    entry.state = InviteState::Revoked;
                    InviteRecord {
                        capability_hash,
                        inviter_id: entry.inviter_id,
                        expires_at: entry.expires_at,
                        state: PersistedInviteState::Revoked,
                        created_at: entry.created_at,
                    }
                }
                // 不认识的串是 no-op（调用方可以 fire-and-forget），也没必要去动库。
                // 报 true：没有需要持久化的东西，不该让调用方以为落盘失败了。
                None => return true,
            }
        };
        self.store.upsert(record).await
    }

    /// 清除已过期条目（lazy 调用即可）。
    pub async fn prune_expired(&self, now: u64) {
        self.invites
            .lock()
            .expect("registry lock")
            .retain(|_, e| now < e.expires_at);
        self.store.prune_expired(now).await;
    }
}

/// 邀请列表条目（发起方视角）。**不含 capability，也不含邀请串** —— 明文不出注册表，
/// 重启后连原始链接都拼不回来（design D4），列表只显示元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteSummary {
    /// `sha256(capability)`：撤销时用它定位，UI 只当不透明 ID 用。
    pub capability_hash: [u8; 32],
    pub created_at: u64,
    pub expires_at: u64,
    /// 已被消费（仍在列表里显示，直到过期）。
    pub consumed: bool,
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
        .expect("夹具带地址")
    }

    /// 前缀匹配必须大小写不敏感 —— **这条曾经是「前缀必须全小写」，被现实推翻**。
    ///
    /// GitHub Pages 项目站的路径段就是仓库名 `SwarmDrop`，带大写且**不能改小写**
    /// （Pages 路径区分大小写，`/swarmdrop/` 是 404）。所以不变量只能是「匹配不敏感」，
    /// 而不是「常量全小写」。
    #[test]
    fn prefix_matching_is_case_insensitive() {
        assert!(
            ACCEPTED_URL_PREFIXES.contains(&INVITE_URL_PREFIX),
            "主前缀必须在受理列表里，否则自己生成的邀请自己解不开"
        );

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let link = invite.encode(&sk);

        for (label, variant) in [
            ("原样", link.clone()),
            ("整串小写", link.to_ascii_lowercase()),
            ("整串大写", link.to_ascii_uppercase()),
        ] {
            let decoded = PairInvite::decode(&variant)
                .unwrap_or_else(|e| panic!("{label}形态应当能解码 —— 实际 {e}"));
            assert_eq!(decoded, invite, "{label}形态解出的应当是同一份邀请");
        }
    }

    /// **域名迁移不该打断在途邀请。**
    ///
    /// 邀请活 24h，用户手上的 App 版本也不会同一秒更新，所以迁移期两种链接会同时在外面
    /// 飘。这条钉住「非主前缀的链接照样解得开」—— 少了它，迁移当天所有在途邀请一起失效，
    /// 症状还是最难自查的那种：链接看着好好的，点开说不是配对邀请。
    #[test]
    fn accepts_every_listed_prefix() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let payload = invite
            .encode(&sk)
            .strip_prefix(INVITE_URL_PREFIX)
            .expect("生成用的就是主前缀")
            .to_owned();

        for p in ACCEPTED_URL_PREFIXES {
            let link = format!("{p}{payload}");
            let decoded = PairInvite::decode(&link)
                .unwrap_or_else(|e| panic!("受理列表里的前缀应当能解码: {p} —— 实际 {e}"));
            assert_eq!(decoded, invite, "解出来的应当是同一份邀请: {p}");
        }
    }

    #[test]
    fn roundtrip_canonical_url() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let s = invite.encode(&sk);
        assert!(s.starts_with(INVITE_URL_PREFIX));
        assert!(!s.ends_with('='), "base32 编码不应带 padding");
        assert_eq!(PairInvite::decode(&s).unwrap(), invite);
    }

    /// 单一载体的核心不变量：**整串大写化后仍是同一份邀请**。
    ///
    /// 二维码内容就是这个大写串（走 QR alphanumeric），所以这条一旦破，
    /// 要么二维码扫不出来，要么得退回「链接 + 二维码两种编码」的旧形态。
    #[test]
    fn uppercased_form_decodes_identically() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let s = invite.encode(&sk);
        let upper = s.to_ascii_uppercase();
        assert_ne!(upper, s, "canonical 应含小写字符，否则这条测试没在测东西");
        assert_eq!(PairInvite::decode(&upper).unwrap(), invite);
    }

    /// 剪贴板与 IM 的真实输入很少是干净的一行。
    #[test]
    fn extracts_from_dirty_text() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let s = invite.encode(&sk);

        for wrapped in [
            format!("  {s}  "),
            format!("{s}\n"),
            format!("看看这个 {s} 用它配对"),
            format!("张三：{s}。"),
            format!("<{s}>"),
            format!("{}\n{s}", "前面还有一行文字"),
        ] {
            assert_eq!(
                PairInvite::decode(&wrapped).unwrap(),
                invite,
                "未能从 {wrapped:?} 提取邀请"
            );
        }
    }

    /// 逐字节翻转整条 wire：**要么被拒，要么受保护字段一字未变**。
    ///
    /// V1 时代这条断言的是「除签名外每个字节被改都必须拒」。地址提示移出签名覆盖范围后
    /// 那句话不再成立——改 hints 区的字节本来就该照常解开。但把断言弱化成「大部分要拒」
    /// 会让它失去区分力，所以换成一条**更强**的不变量：解码成功就意味着改动落在 hints 上，
    /// 那么六个受保护字段必须逐个不变。
    ///
    /// 这条红了说明签名覆盖范围漏了某个字段——那才是真正的降级面。
    #[test]
    fn tampering_either_fails_or_leaves_protected_fields_intact() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::LocalOnly);
        let s = invite.encode(&sk);
        let bytes = BASE32_NOPAD
            .decode(extract_payload(&s).unwrap().as_bytes())
            .unwrap();

        let mut survived = 0usize;
        for i in 0..bytes.len() {
            let mut tampered = bytes.clone();
            tampered[i] ^= 0x01;
            let text = format!("{INVITE_URL_PREFIX}{}", BASE32_NOPAD.encode(&tampered));
            let Ok(got) = PairInvite::decode(&text) else {
                continue;
            };
            survived += 1;
            assert_eq!(got.capability, invite.capability, "第 {i} 字节");
            assert_eq!(got.inviter.id, invite.inviter.id, "第 {i} 字节");
            assert_eq!(got.expires_at, invite.expires_at, "第 {i} 字节");
            assert_eq!(got.transport_policy, invite.transport_policy, "第 {i} 字节");
            assert_eq!(got.display_name, invite.display_name, "第 {i} 字节");
            assert_eq!(got.display_platform, invite.display_platform, "第 {i} 字节");
        }
        // 非空断言：hints 区确实存在、确实有字节改了还能解开。否则上面整个循环是空转，
        // 而空转的循环看起来和「全都拒了」一模一样。
        assert!(
            survived > 0,
            "没有任何一个字节改动后仍能解码——hints 区似乎也进了签名，本 change 白做了"
        );
    }

    /// 地址提示条数必须有上限 —— 它不在签名覆盖范围内，条数是攻击者可控的。
    ///
    /// 没有这条闸时，一条被改写的邀请就是一台指哪打哪的连接洪泛发生器：受邀方会把全部提示
    /// 经 `Endpoint::add_addrs` 登记并拨号。见 [`MAX_ADDR_HINTS`]。
    #[test]
    fn too_many_address_hints_are_rejected() {
        let sk = SecretKey::generate();
        let flood: Vec<Addr> = (0..MAX_ADDR_HINTS + 1)
            .map(|i| {
                format!("/ip4/198.51.100.{}/tcp/{}", i % 256, 4000 + i)
                    .parse()
                    .unwrap()
            })
            .collect();
        let encoded = PairInvite::generate(
            &sk,
            flood,
            TransportPolicy::Auto,
            "书房的 Mac".into(),
            "macos".into(),
            1_700_000_000,
        )
        .expect("夹具带地址")
        .encode(&sk);
        assert!(
            matches!(
                PairInvite::decode(&encoded),
                Err(InviteParseError::Verify(_))
            ),
            "超过 {MAX_ADDR_HINTS} 条地址提示必须被拒"
        );
    }

    /// **验签通过、字段完好、却一条地址都没有** —— 这是本模块最坏的输出形态，任何篡改都
    /// 不该产出它。
    ///
    /// 用字节翻转搜索而不是手工构造损坏结构：`CompactAddrs` 的内部字段对本 crate 不可见，
    /// 而这条规则要防的恰恰是「提示区被改坏 → `unpack` 逐条丢弃 → 丢光了也没人说话」。
    /// 那样的邀请拨不动任何东西，用户要等到 RPC 超时才看到一句与病因无关的
    /// 「发送配对请求失败」。
    ///
    /// ⚠️ **夹具必须是「单条、且带 certhash 下标」的地址，翻转也必须覆盖 8 个 bit 位。**
    /// 第一版用的是 `test_invite`（两条裸 TCP 地址、只翻最低位），去掉被测判断照样绿 ——
    /// 两条地址里杀不死全部路径，裸 TCP 又没有下标可打坏，`0x01` 还只能让下标 0↔1 互换、
    /// 始终落在表内。那样的测试看起来在守规则，实际一个字节都没守住。
    #[test]
    fn no_tampering_yields_a_zero_address_invite() {
        let sk = SecretKey::generate();
        let invite = PairInvite::generate(
            &sk,
            vec![
                "/ip4/192.168.1.10/udp/4004/quic-v1/webtransport\
                 /certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g\
                 /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
                    .parse()
                    .unwrap(),
            ],
            TransportPolicy::Auto,
            "书房的 Mac".into(),
            "macos".into(),
            1_700_000_000,
        )
        .expect("夹具带地址");
        let bytes = BASE32_NOPAD
            .decode(extract_payload(&invite.encode(&sk)).unwrap().as_bytes())
            .unwrap();

        for i in 0..bytes.len() {
            for bit in 0..8u8 {
                let mut tampered = bytes.clone();
                tampered[i] ^= 1 << bit;
                let text = format!("{INVITE_URL_PREFIX}{}", BASE32_NOPAD.encode(&tampered));
                if let Ok(got) = PairInvite::decode(&text) {
                    assert!(
                        !got.inviter.addrs.is_empty(),
                        "第 {i} 字节 bit {bit} 被篡改后解出了一条零地址邀请——它编得出、\
                         扫得动、复制得走，唯独没有任何东西可拨"
                    );
                }
            }
        }
    }

    /// 生成侧的零地址闸 —— 与上面那条 `no_tampering_yields_a_zero_address_invite` 是
    /// **同一条不变量的两端**：那条守「本机不接受零地址邀请」，这条守「本机不产出它」。
    ///
    /// 缺了这一半时的表现极难归因：发起方拿到一张完全正常的二维码，受邀方扫了才报错，
    /// 而两人手上的信息对不上。浏览器端最容易撞上（可拨地址全部来自 relay reservation，
    /// 落定之前是空集而非「少几条」）。
    #[test]
    fn generating_without_any_addr_is_refused() {
        let sk = SecretKey::generate();
        assert!(
            matches!(
                PairInvite::generate(
                    &sk,
                    vec![],
                    TransportPolicy::Auto,
                    "书房的 Mac".into(),
                    "macos".into(),
                    1_700_000_000,
                ),
                Err(NoDialableAddrs)
            ),
            "零地址邀请必须在生成侧就被拒，而不是编出来交给用户"
        );
    }

    /// 把闸挡下的那种邀请**硬造出来**，确认它正是解码侧会拒的那一种。
    ///
    /// 上一条只证明「生成被拒了」，不能证明「被拒的是对的东西」——万一解码侧其实收得下
    /// 零地址邀请，那道闸就是纯粹的多余限制。这条经 `addrs_mut`（不需要私钥的合法路径）
    /// 绕过生成侧，直接验两端咬合。
    #[test]
    fn the_refused_shape_is_exactly_what_decode_rejects() {
        let sk = SecretKey::generate();
        let mut signed = test_invite(&sk, TransportPolicy::Auto).sign(&sk);
        signed.addrs_mut().clear();
        assert!(
            matches!(
                PairInvite::decode(&signed.encode()),
                Err(InviteParseError::Verify(_))
            ),
            "生成侧挡下的形状，解码侧也必须拒 —— 否则那道闸是多余的"
        );
    }

    /// 地址提示可被任意方**替换**，邀请仍然有效。
    ///
    /// 与上面那条逐字节翻转互补：那条证明「改了也不会伪造」，这条证明「改了确实还能用」
    /// ——后者才是 [`SignedInvite::addrs_mut`] 与「按码面裁剪不需要私钥」的立足点。
    #[test]
    fn address_hints_can_be_replaced_without_the_key() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let signed = invite.sign(&sk);

        let bytes = BASE32_NOPAD
            .decode(extract_payload(&signed.encode()).unwrap().as_bytes())
            .unwrap();
        let InviteWire::V2(wire) = postcard::from_bytes(&bytes).unwrap();
        // 换成一组完全不同的地址，**不碰 core 与 signature**
        let forged = postcard::to_stdvec(&InviteWire::V2(InviteV2 {
            core: wire.core.clone(),
            signature: wire.signature,
            hints: compact::pack(&["/ip4/198.51.100.7/tcp/9999".parse().unwrap()]),
        }))
        .unwrap();

        let text = format!("{INVITE_URL_PREFIX}{}", BASE32_NOPAD.encode(&forged));
        let got = PairInvite::decode(&text).expect("换掉地址提示不该让邀请失效");
        assert_eq!(got.capability, invite.capability);
        assert_eq!(got.expires_at, invite.expires_at);
        assert_eq!(
            got.inviter.addrs,
            vec!["/ip4/198.51.100.7/tcp/9999".parse::<Addr>().unwrap()],
            "地址提示应当是被替换后的那组"
        );
    }

    #[test]
    fn wrong_prefix_and_garbage_rejected() {
        // 完全无关的文本
        assert!(matches!(
            PairInvite::decode("blobabcdefg"),
            Err(InviteParseError::Kind)
        ));
        // 旧形态（`sd:` + base64url）必须**不再**被接受——本期不留兼容读取路径
        assert!(matches!(
            PairInvite::decode("sd:AAAA"),
            Err(InviteParseError::Kind)
        ));
        // 别的域名下的同名路径不认
        assert!(matches!(
            PairInvite::decode("https://evil.example/P/#AAAA"),
            Err(InviteParseError::Kind)
        ));
        // 前缀对但没有 payload
        assert!(matches!(
            PairInvite::decode(INVITE_URL_PREFIX),
            Err(InviteParseError::Encoding(_))
        ));
        // 前缀对、base32 合法、内容不是 postcard wire
        let junk = format!("{INVITE_URL_PREFIX}{}", BASE32_NOPAD.encode(&[9u8; 80]));
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

    /// 不落盘的注册表（这些用例只关心内存态语义；落盘侧另有 storage-sql / web 的测试）。
    fn memory_registry() -> InviteRegistry {
        InviteRegistry::default()
    }

    #[tokio::test]
    async fn registry_ttl_and_capability() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = memory_registry();
        reg.register(&invite, 1_700_000_000).await;
        // 错 capability
        assert_eq!(
            reg.try_consume(&[0u8; 16], invite.expires_at - INVITE_TTL_SECS)
                .await,
            Err(InviteRejectReason::Unknown)
        );
        // 过期
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at).await,
            Err(InviteRejectReason::Expired)
        );
        // 正常消费
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS)
                .await,
            Ok(())
        );
        // 重复消费拒
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS)
                .await,
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// 一次性语义的核心：真并发抢同一份邀请，恰有一胜。
    ///
    /// 用真线程（而非 tokio 并发任务）是刻意的 —— 要抢的是那把 `std::sync::Mutex`，
    /// 单线程 executor 下的「并发」压根不会竞争它。
    #[test]
    fn concurrent_double_spend_single_winner() {
        use std::sync::Arc;
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = Arc::new(memory_registry());
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(reg.register(&invite, 1_700_000_000));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let reg = reg.clone();
            let cap = invite.capability;
            let now = invite.expires_at - INVITE_TTL_SECS;
            handles.push(std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap()
                    .block_on(reg.try_consume(&cap, now))
                    .is_ok()
            }));
        }
        let wins: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum();
        assert_eq!(wins, 1, "并发双花必须恰有一胜");
    }

    #[tokio::test]
    async fn revoke_blocks_consume() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = memory_registry();
        reg.register(&invite, 1_700_000_000).await;
        reg.revoke(&invite.capability).await;
        assert_eq!(
            reg.try_consume(&invite.capability, invite.expires_at - INVITE_TTL_SECS)
                .await,
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// 撤销侧（三端 UI）手上只有邀请串，capability 要经解码取回。这条钉死
    /// 「编解码往返后仍索引到同一条记录」——`PairingManager::revoke_invite` 的不变量。
    #[tokio::test]
    async fn revoke_via_decoded_invite_string_blocks_consume() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let reg = memory_registry();
        reg.register(&invite, 1_700_000_000).await;

        let decoded = PairInvite::decode(&invite.encode(&sk)).unwrap();
        reg.revoke(&decoded.capability).await;

        assert_eq!(
            reg.check(&invite.capability, invite.expires_at - INVITE_TTL_SECS),
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// **撤销必须跨重启生效。**
    ///
    /// 早期实现的落盘侧是「删行」，而端口方法不返回错误 —— 删行失败时库里仍是 `Pending`，
    /// 重启后读回来那条邀请就重新可用了。撤销是用户明确表达「不要它继续可用」的动作，
    /// fail-open 在这里不可接受，所以改成写 `Consumed`。
    #[tokio::test]
    async fn revoke_survives_restart() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());

        let before = InviteRegistry::new(store.clone());
        before.register(&invite, now).await;
        before.revoke(&invite.capability).await;

        // 「重启」：新注册表，同一份落盘
        let after = InviteRegistry::new(store.clone());
        after.load(now).await;
        assert_eq!(
            after.try_consume(&invite.capability, now).await,
            Err(InviteRejectReason::Unavailable),
            "撤销过的邀请在重启后必须仍然不可用"
        );
        assert!(
            after.list_active(now).is_empty(),
            "撤销过的邀请不该出现在列表里"
        );
    }

    /// 落盘往返：注册 → 从同一 store 冷启动 → 状态与一次性语义都还在。
    #[tokio::test]
    async fn load_restores_state_across_restart() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());

        let before = InviteRegistry::new(store.clone());
        before.register(&invite, now).await;
        assert_eq!(before.try_consume(&invite.capability, now).await, Ok(()));

        // 「重启」：新注册表，同一份落盘
        let after = InviteRegistry::new(store.clone());
        after.load(now).await;
        assert_eq!(
            after.try_consume(&invite.capability, now).await,
            Err(InviteRejectReason::Unavailable),
            "已消费状态必须跨重启保留，否则同一邀请能被用第二次"
        );
    }

    /// 过期条目不进内存表，也从库里清掉。
    #[tokio::test]
    async fn load_drops_expired_records() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let store = Arc::new(TestStore::default());
        let before = InviteRegistry::new(store.clone());
        before
            .register(&invite, invite.expires_at - INVITE_TTL_SECS)
            .await;

        let after = InviteRegistry::new(store.clone());
        after.load(invite.expires_at).await; // 恰好到期
        assert!(after.list_active(invite.expires_at).is_empty());
        assert!(
            store.records.lock().unwrap().is_empty(),
            "过期记录应从库里清掉"
        );
    }

    #[tokio::test]
    async fn list_active_reports_consumed_and_sorts_newest_first() {
        let sk = SecretKey::generate();
        let older = test_invite(&sk, TransportPolicy::Auto);
        let newer = test_invite(&sk, TransportPolicy::Auto);
        let now = older.expires_at - INVITE_TTL_SECS;
        let reg = memory_registry();
        reg.register(&older, now).await;
        reg.register(&newer, now + 10).await;
        reg.try_consume(&older.capability, now).await.unwrap();

        let list = reg.list_active(now + 20);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].created_at, now + 10, "最近生成的排最前");
        assert!(!list[0].consumed);
        assert!(list[1].consumed, "已消费的仍在列表里，显示为已使用");

        // 撤销后从列表消失
        reg.revoke(&newer.capability).await;
        assert_eq!(reg.list_active(now + 20).len(), 1);
    }

    /// **写穿窗口内一切判定仍然拒绝。**
    ///
    /// CAS 成功、写穿还没落地的那段时间里，库里是 `register` 写下的 `Pending`。
    /// 判定只读内存（锁内已翻好），所以这个窗口对安全性没有影响 —— 这条把它钉住。
    /// 没有这个栅栏桩的话，`TestStore::upsert` 立刻完成，窗口在测试里根本不存在。
    /// **刻意用 multi_thread**：单线程 runtime 下「spawn 的任务被 poll 一次就撞到闸」这个
    /// 巧合会让 `yield_now()` 式的猜时序也能过，等于把护栏调松。多线程下没有这个巧合 ——
    /// 实测把握手换回 `yield_now()` + `notify_waiters()`，这条测试 12/12 永久挂住。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn mid_writeback_window_stays_fail_closed() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());
        let reg = Arc::new(InviteRegistry::new(store.clone()));
        reg.register(&invite, now).await;

        // 让后续写穿挂住，把窗口撑开
        let gate = store.hold_writes();
        let hash = capability_hash(&invite.capability);
        let consuming = {
            let reg = reg.clone();
            let cap = invite.capability;
            tokio::spawn(async move { reg.try_consume(&cap, now).await })
        };
        // 等它真的进到写穿（内存已置 Consumed，库里还是 Pending）。
        // 这里必须等回执而不是 yield_now()——后者是在猜调度，见 TestStore::hold_writes。
        store.entered.notified().await;

        // 断言顺序：先读「用户/库看到什么」，再调会改状态的路径。反了就会读到
        // 第二次消费之后的状态，断言假红。
        assert_eq!(
            store.state_of(&hash),
            Some(PersistedInviteState::Pending),
            "窗口内库里应当还是 register 写下的 Pending"
        );
        assert_eq!(
            reg.check(&invite.capability, now),
            Err(InviteRejectReason::Unavailable),
            "窗口内 check 必须已经拒绝"
        );
        assert_eq!(
            reg.try_consume(&invite.capability, now).await,
            Err(InviteRejectReason::Unavailable),
            "窗口内第二路消费必须拒绝（一次性语义不依赖库）"
        );

        gate.notify_one();
        assert_eq!(consuming.await.unwrap(), Ok(()), "第一路应当成功");
    }

    /// **消费的写穿失败必须 fail-closed。**
    ///
    /// 否则：库里仍是 `Pending` → 重启 `load` 放回内存 → 同一份一次性凭证被消费第二次，
    /// 而且列表会把它显示成「等待对方使用」，用户看不到任何痕迹、不会想去撤销
    /// （唯一的补救信号恰好在这个失败模式下消失）。所以这里宁可让配对诚实地失败 ——
    /// 调用点的 `try_consume` 排在 `responder.send(Success)` 之前，此刻还没告诉对端成功。
    #[tokio::test]
    async fn consume_fails_closed_when_writeback_fails() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());
        let reg = InviteRegistry::new(store.clone());
        reg.register(&invite, now).await;

        store.fail_writes();
        assert_eq!(
            reg.try_consume(&invite.capability, now).await,
            Err(InviteRejectReason::NotPersisted),
            "写穿失败时消费必须报错，不能默默成功"
        );
    }

    /// **撤销的写穿失败要能被调用方看到。**
    ///
    /// 它不像消费那样能 fail-closed（撤销是清理路径，没有可中止的下游动作），
    /// 但调用方必须有能力知道「这次撤销没落盘、重启后邀请会复活」，好告诉用户。
    #[tokio::test]
    async fn revoke_reports_writeback_failure() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());
        let reg = InviteRegistry::new(store.clone());
        reg.register(&invite, now).await;

        store.fail_writes();
        assert!(
            !reg.revoke(&invite.capability).await,
            "撤销未落盘时必须报 false"
        );
        // 本进程内仍然生效（内存已标 Revoked）
        assert_eq!(
            reg.check(&invite.capability, now),
            Err(InviteRejectReason::Unavailable)
        );
    }

    /// `load` 不得把内存里的终态降级回 `Pending`。
    ///
    /// 当前只在全新注册表上调用一次，所以不可达；但那是个**未被强制**的约定，
    /// 而代价只是一次状态比较。审查实测过：无保护时重复 `load` 会让已消费的邀请复活。
    #[tokio::test]
    async fn load_never_downgrades_memory_state() {
        use std::sync::Arc;

        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let now = invite.expires_at - INVITE_TTL_SECS;
        let store = Arc::new(TestStore::default());
        let reg = InviteRegistry::new(store.clone());
        reg.register(&invite, now).await;
        // 让库停在 Pending，内存推进到 Consumed
        store.fail_writes();
        let _ = reg.try_consume(&invite.capability, now).await;

        reg.load(now).await;

        assert_eq!(
            reg.try_consume(&invite.capability, now).await,
            Err(InviteRejectReason::Unavailable),
            "load 把库里的 Pending 盖回内存会让已消费的邀请复活"
        );
    }

    /// 端口的内存桩：只验注册表侧的读写时序，不碰真实存储。
    ///
    /// 两个可注入的行为，用来测那些「正常路径下看不见」的性质：
    /// - `fail_writes`：让写穿静默失败 —— 复现「内存已改、库里还是 `register` 写的 Pending」
    /// - `hold_writes`：让写穿挂在栅栏上 —— 把「CAS 已成功、写穿未完成」那个窗口撑开
    #[derive(Default)]
    struct TestStore {
        records: Mutex<HashMap<[u8; 32], InviteRecord>>,
        fail_writes: std::sync::atomic::AtomicBool,
        hold_writes: Mutex<Option<Arc<tokio::sync::Notify>>>,
        /// 写穿「已进闸」的回执，让测试不必猜调度时序（见 `hold_writes` 的文档）。
        entered: tokio::sync::Notify,
    }

    impl TestStore {
        fn fail_writes(&self) {
            self.fail_writes
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// 后续写穿都挂起，直到返回的 `Notify` 被 `notify_one()`。
        ///
        /// 配套的 `entered` 是必需的，不是方便：测试必须先 `entered.notified().await`
        /// 确认写穿真的进了闸，再做窗口内的断言。用 `yield_now()` 猜时序会两头出错——
        /// 断言可能跑在 CAS 之前（假红），放行也可能跑在挂起之前而丢失（永久挂住）。
        ///
        /// 两个 `Notify` 都用 `notify_one()`：它**存许可**，所以先放行后挂起也能被消费，
        /// 握手不依赖两个任务谁先被调度。`notify_waiters()` 不存许可，这里不能用。
        ///
        /// 闸是无差别的（挂住后续所有写），靠**在 `register` 之后才开闸**避免把注册本身
        /// 卡死——只要窗口内不再触发别的写穿，这比按状态选择性挂闸简单。
        fn hold_writes(&self) -> Arc<tokio::sync::Notify> {
            let gate = Arc::new(tokio::sync::Notify::new());
            *self.hold_writes.lock().unwrap() = Some(gate.clone());
            gate
        }

        fn state_of(&self, hash: &[u8; 32]) -> Option<PersistedInviteState> {
            self.records.lock().unwrap().get(hash).map(|r| r.state)
        }
    }

    #[async_trait::async_trait]
    impl InviteStore for TestStore {
        async fn load_all(&self) -> Vec<InviteRecord> {
            self.records.lock().unwrap().values().cloned().collect()
        }
        async fn upsert(&self, record: InviteRecord) -> bool {
            // 锁不能跨 await，所以先取出 gate 再挂
            let gate = self.hold_writes.lock().unwrap().clone();
            if let Some(gate) = gate {
                // 回执必须发在挂起之前，否则测试等不到「已进闸」
                self.entered.notify_one();
                gate.notified().await;
            }
            if self.fail_writes.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            self.records
                .lock()
                .unwrap()
                .insert(record.capability_hash, record);
            true
        }
        async fn remove(&self, capability_hash: [u8; 32]) -> bool {
            if self.fail_writes.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            self.records.lock().unwrap().remove(&capability_hash);
            true
        }
        async fn prune_expired(&self, now: u64) {
            self.records
                .lock()
                .unwrap()
                .retain(|_, r| now < r.expires_at);
        }
    }

    /// wire 契约锁定：V2 的版本判别码与签名对象。
    ///
    /// **本测试失败 = wire 契约被改动**，已发出的邀请串将无法解析。禁止随手「修」它，
    /// 先回看 [`InviteV2`] 的改动。
    #[test]
    fn wire_v2_signs_the_domain_tagged_core() {
        let sk = SecretKey::generate();
        let mut invite = test_invite(&sk, TransportPolicy::LocalOnly);
        invite.capability = [0x22; 16];
        let bytes = BASE32_NOPAD
            .decode(
                extract_payload(&invite.encode(&sk))
                    .unwrap()
                    .to_ascii_uppercase()
                    .as_bytes(),
            )
            .unwrap();

        assert_eq!(bytes[0], 0x00, "V2 判别码必须是 0x00（单变体 enum）");
        let InviteWire::V2(wire) = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(
            &wire.core[..16],
            &[0x22; 16],
            "capability 必须是 core 的第一个字段"
        );
        assert!(
            invite
                .inviter
                .id
                .verify(&signing_message(&wire.core), &wire.signature),
            "签名对象必须是 域分隔标签 ‖ core"
        );
    }

    /// 域分隔标签必须真的在签名对象里。
    ///
    /// 没有它，同样一组字段的签名可以搬到别的版本上继续有效——V1 靠「签名覆盖含 enum
    /// 判别码在内的全部前置字节」达到同一效果，那是位置约定的副产品；换成显式标签后，
    /// **这条测试是它唯一的看守**。裸 core 验签通过 = 标签被拿掉了。
    #[test]
    fn signature_does_not_verify_over_the_bare_core() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let bytes = BASE32_NOPAD
            .decode(extract_payload(&invite.encode(&sk)).unwrap().as_bytes())
            .unwrap();
        let InviteWire::V2(wire) = postcard::from_bytes(&bytes).unwrap();
        assert!(
            !invite.inviter.id.verify(&wire.core, &wire.signature),
            "不带域分隔标签的 core 不该验得过"
        );
    }

    /// 裁掉地址后**不重新签名**，邀请仍然有效——`compose::fit_to_scannable` 的立足点。
    #[test]
    fn trimming_addresses_needs_no_key() {
        let sk = SecretKey::generate();
        let invite = test_invite(&sk, TransportPolicy::Auto);
        let mut signed = invite.sign(&sk);
        assert_eq!(signed.addrs().len(), 2);

        signed.addrs_mut().pop();
        let got = PairInvite::decode(&signed.encode()).expect("裁剪后仍须验签通过");
        assert_eq!(got.inviter.addrs.len(), 1);
        assert_eq!(got.capability, invite.capability);
        assert_eq!(got.expires_at, invite.expires_at);
        assert_eq!(got.transport_policy, invite.transport_policy);
    }
}

/// `extract_payload` 的 UTF-8 边界安全。
///
/// 它用「`to_ascii_lowercase()` 后的串」上找到的字节下标去切**原串** —— 这一步之所以成立，
/// 全靠 `to_ascii_lowercase` **逐字节等长**（只动 ASCII，非 ASCII 字节原样保留）。
///
/// **不要把它换成 `to_lowercase()`**：那是 Unicode 感知的，会改变字节长度
/// （`ẛ` → `ṡ`、`İ` → `i̇` 都变长），下标随即错位，最好的情况是切出错误子串，
/// 最坏的情况是切在多字节字符中间直接 panic。这个模块就是钉住这一点的。
#[cfg(test)]
mod utf8_safety {
    use super::*;
    use swarmdrop_net_base::SecretKey;

    #[test]
    fn multibyte_neighbours_do_not_break_extraction() {
        let sk = SecretKey::generate();
        let invite = PairInvite::generate(
            &sk,
            vec!["/ip4/192.168.1.10/tcp/4001".parse().unwrap()],
            TransportPolicy::Auto,
            "书房 Mac".into(),
            "macos".into(),
            1_700_000_000,
        )
        .expect("夹具带地址");
        let canonical = invite.encode(&sk);

        let cases = [
            format!("看看这个👉{canonical}"),
            format!("{canonical}👈用它配对"),
            format!("「{canonical}」"),
            format!("Ünïcödé {canonical} ÅÄÖ"),
            format!("İstanbul {canonical}"), // 土耳其点 I：大小写映射的经典雷区
            format!("ẛ{canonical}"),         // 长 s 带点：Unicode 小写化会变长
            format!("𝕊𝕨𝕒𝕣𝕞{canonical}"),     // 4 字节字符
            format!("中文{canonical}中文"),
        ];
        for input in cases {
            let decoded =
                PairInvite::decode(&input).unwrap_or_else(|e| panic!("从 {input:?} 提取失败: {e}"));
            assert_eq!(decoded, invite, "从 {input:?} 提取出的邀请不对");
        }
    }
}
