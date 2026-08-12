//! 地址列表的紧凑二进制表示（openspec: `invite-wire-v2` / capability `addr-compact-codec`）。
//!
//! multiaddr 的二进制形态把**节点级数据按地址重复**：certhash 是节点级的（当前 + 下一张，
//! 14 天轮换），却在每条 WebTransport 地址上各带一份；relay 身份是 bootstrap 级的，却在每条
//! circuit 地址上各带一份。实测单段成本：
//!
//! | 段 | 字节 |
//! |---|---:|
//! | `/certhash/<sha256 multihash>` | **37** |
//! | `/p2p/<id>` | **41** |
//! | `/ip4/…` · `/tcp/…` · `/udp/…` · `/quic-v1` 等 | 2–5 |
//!
//! 于是一条 WebTransport 地址 87 字节里 74 字节是 certhash，一条 WebTransport 基址的 circuit
//! 地址 130 字节里 115 字节是 certhash + relay 身份。本模块把这两样各建一张去重表、路径按
//! 下标引用，満配桌面的地址区从 **663 → 267 字节**。
//!
//! 它的第一消费者是配对邀请：邀请靠二维码承载，码面密度是硬约束，地址多到一定程度就扫不动。
//!
//! # 唯一的硬要求是**逐字节还原**
//!
//! 判据用字节而非语义相等：地址的全部价值就在那串字节上。还原出一条「看起来对但少一个
//! certhash」的地址不会报错，只会让对端拨不通——比「没有这条地址」更难归因。
//!
//! 由此推出本模块的两条纪律：
//!
//! 1. **认不出就 [`CompactPath::Raw`]，绝不猜。** 只有完整匹配已建模形态的地址才走结构化
//!    路径；任何一段不认识、或段序不符预期，整条原样搬。新传输不改本模块也能进邀请，
//!    只是没有体积收益。
//! 2. **不依赖外部上下文。** [`pack`] / [`unpack`] 只吃地址列表。曾设想「省掉 `/p2p/<本机>`
//!    后缀、由邀请的 `inviter_id` 补回」，查证后取消：那个形态根本不出现在
//!    `Endpoint::watch_addrs().dialable()` 里（带自身身份的那条由 `Actor::circuit_addr_for`
//!    拼出，唯一消费点是节点状态弹窗的诊断展示值）。这类优化会让同一份编码在不同调用方
//!    手里还原出不同结果，而那正是上面那条硬要求要防的。
//!
//! # 表里存原字节，不存「解读后的摘要」
//!
//! certhash 存 multihash 原字节而非 32 字节裸摘要，relay 存 `PeerId` 原字节而非 ed25519
//! 公钥。剥掉头部每项能再省 6 字节、全表合计十几字节，代价是「非 sha2-256 的 certhash」
//! 或「非 ed25519 的 peer id」会被静默改写成另一个值——那是本模块唯一不可接受的失败形态。
//! 真正的收益来自**去重**（3 条 WebTransport 地址共用 2 个 certhash：222 → 76 字节），
//! 与剥不剥头无关。

use libp2p_identity::PeerId;
use multiaddr::multihash::Multihash;
use multiaddr::{Multiaddr, Protocol};
use serde::{Deserialize, Serialize};

use crate::Addr;

/// 地址的主机段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Host {
    Ip4([u8; 4]),
    Ip6([u8; 16]),
}

/// 端口之后的传输段组合。
///
/// 每个变体对应一组**固定的**段序列，还原时逐段拼回（见 [`CompactAddrs::push_direct`]）。
/// 变体不是「传输名」而是「段模板」——`WebTransport` 展开成 `/quic-v1/webtransport` 两段，
/// 这也是为什么它必须排在 `QuicV1` 之前匹配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Wire {
    /// `/tcp/<port>`
    Tcp,
    /// `/udp/<port>/quic-v1`
    QuicV1,
    /// `/udp/<port>/quic-v1/webtransport`
    WebTransport,
    /// `/udp/<port>/webrtc-direct`
    WebRtcDirect,
}

impl Wire {
    /// 这一档传输是否可以携带 `/certhash` 段。
    ///
    /// 不是装饰性判断：TCP 地址后面跟着 certhash 是个还原不回去的形态（拼不出原段序），
    /// 必须在识别阶段就落 `Raw`。
    const fn carries_certhash(self) -> bool {
        matches!(self, Self::WebTransport | Self::WebRtcDirect)
    }
}

/// 一条直连路径。`certs` 是 [`CompactAddrs::certhashes`] 的下标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Direct {
    host: Host,
    port: u16,
    wire: Wire,
    certs: Vec<u16>,
}

/// 一条地址的紧凑形态。
///
/// ⚠️ **本类型必须保持非递归。** 曾经 `Circuit.base` 是 `Box<CompactPath>`，于是这份 wire
/// 可以嵌套任意深 —— 而 postcard 反序列化跑在**验签之前**（`PairInvite::decode`），一条
/// 一万层嵌套的邀请文本就能让解码栈溢出。栈溢出是 abort 不是 `Err`，捕获不了，进程直接死；
/// 而邀请文本是从剪贴板**自动**读的，等于任何人发来一段文字就能杀掉 App。
///
/// 递归当时根本没被用到：circuit 的基址只可能是直连路径。现在这条由类型保证 ——
/// `base: Direct` 不是 `Box<CompactPath>`，深度恒为 1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum CompactPath {
    Direct(Direct),
    /// 中继路径：`<base>/p2p/<relay>/p2p-circuit[/webrtc]`。
    ///
    /// `relay` 是 [`CompactAddrs::relays`] 的下标；`hole_punch` 对应尾部那个 `/webrtc` 段
    /// ——它在语义上是「circuit 只用于信令、数据面直连」，与纯中继是两回事
    /// （见 [`Addr::dial_tier`](crate::Addr::dial_tier)）。
    Circuit {
        relay: u16,
        base: Direct,
        hole_punch: bool,
    },
    /// 认不出的形态：multiaddr 原字节，原样搬运。
    Raw(Vec<u8>),
}

/// 一组地址的紧凑表示。构造与还原只经 [`pack`] / [`unpack`]。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactAddrs {
    /// 出现过的**全部** certhash（multihash 原字节），去重。
    ///
    /// ⚠️ 语义是去重表，**不是「本机的证书」**：WebTransport 基址的 circuit 地址里带的是
    /// *relay 的*证书摘要，与本机那两张无关，但它们进同一张表。
    certhashes: Vec<Vec<u8>>,
    /// 出现过的全部中继身份（`PeerId` 原字节），去重。
    relays: Vec<Vec<u8>>,
    paths: Vec<CompactPath>,
}

impl CompactAddrs {
    /// 携带的路径条数。**在 `unpack` 之前**可问 —— 调用方据此设上限、以及区分
    /// 「本来就没有地址」与「有地址但全都还原不出来」。
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// 一条路径都没有。
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// 识别阶段的产物：还没有 intern 进表的直连路径。
struct DirectShape {
    host: Host,
    port: u16,
    wire: Wire,
    certs: Vec<Multihash<64>>,
}

impl CompactAddrs {
    /// 把一项存进去重表并返回下标；已存在则复用。
    fn intern(table: &mut Vec<Vec<u8>>, item: Vec<u8>) -> Option<u16> {
        let index = match table.iter().position(|existing| *existing == item) {
            Some(index) => index,
            None => {
                table.push(item);
                table.len() - 1
            }
        };
        // 下标存 u16。真实地址集里 certhash 与 relay 都是个位数，越界只可能来自构造异常
        // 的输入——那种输入让整条落 Raw，而不是截断成一个指向别处的下标。
        u16::try_from(index).ok()
    }

    /// 把还原出的直连段追加进 multiaddr。
    fn push_direct(&self, addr: &mut Multiaddr, direct: &Direct) -> Option<()> {
        let Direct {
            host,
            port,
            wire,
            certs,
        } = direct;
        let (port, wire) = (*port, *wire);
        addr.push(match host {
            Host::Ip4(octets) => Protocol::Ip4((*octets).into()),
            Host::Ip6(octets) => Protocol::Ip6((*octets).into()),
        });
        match wire {
            Wire::Tcp => addr.push(Protocol::Tcp(port)),
            Wire::QuicV1 => {
                addr.push(Protocol::Udp(port));
                addr.push(Protocol::QuicV1);
            }
            Wire::WebTransport => {
                addr.push(Protocol::Udp(port));
                addr.push(Protocol::QuicV1);
                addr.push(Protocol::WebTransport);
            }
            Wire::WebRtcDirect => {
                addr.push(Protocol::Udp(port));
                addr.push(Protocol::WebRTCDirect);
            }
        }
        for index in certs {
            let raw = self.certhashes.get(usize::from(*index))?;
            addr.push(Protocol::Certhash(Multihash::from_bytes(raw).ok()?));
        }
        Some(())
    }

    /// 还原一条路径。任何一处对不上返回 `None`——调用方据此跳过这一条。
    fn rebuild(&self, path: &CompactPath) -> Option<Addr> {
        match path {
            CompactPath::Raw(bytes) => Addr::from_bytes(bytes).ok(),
            CompactPath::Direct(direct) => {
                let mut addr = Multiaddr::empty();
                self.push_direct(&mut addr, direct)?;
                Some(Addr::from_multiaddr(addr))
            }
            CompactPath::Circuit {
                relay,
                base,
                hole_punch,
            } => {
                let mut addr = Multiaddr::empty();
                self.push_direct(&mut addr, base)?;
                let raw = self.relays.get(usize::from(*relay))?;
                addr.push(Protocol::P2p(PeerId::from_bytes(raw).ok()?));
                addr.push(Protocol::P2pCircuit);
                if *hole_punch {
                    addr.push(Protocol::WebRTC);
                }
                Some(Addr::from_multiaddr(addr))
            }
        }
    }
}

/// 识别一条直连路径的段序列。不匹配返回 `None`（调用方落 `Raw`）。
fn direct_shape(segments: &[Protocol<'_>]) -> Option<DirectShape> {
    let (host, rest) = match segments {
        [Protocol::Ip4(ip), rest @ ..] => (Host::Ip4(ip.octets()), rest),
        [Protocol::Ip6(ip), rest @ ..] => (Host::Ip6(ip.octets()), rest),
        _ => return None,
    };
    // ⚠️ WebTransport 必须排在 QuicV1 之前：它的段序是 `/quic-v1/webtransport`，两个段同时
    // 在场。判反了会把 `/webtransport` 当成一条多余的段而落 Raw——不是错误，但白丢收益。
    // 同一个陷阱在 `Addr::transport` 与 `Addr::is_webtransport` 的文档里各记了一次。
    let (port, wire, rest) = match rest {
        [Protocol::Tcp(port), rest @ ..] => (*port, Wire::Tcp, rest),
        [
            Protocol::Udp(port),
            Protocol::QuicV1,
            Protocol::WebTransport,
            rest @ ..,
        ] => (*port, Wire::WebTransport, rest),
        [Protocol::Udp(port), Protocol::QuicV1, rest @ ..] => (*port, Wire::QuicV1, rest),
        [Protocol::Udp(port), Protocol::WebRTCDirect, rest @ ..] => {
            (*port, Wire::WebRtcDirect, rest)
        }
        _ => return None,
    };
    let mut certs = Vec::with_capacity(rest.len());
    for segment in rest {
        let Protocol::Certhash(hash) = segment else {
            return None;
        };
        certs.push(*hash);
    }
    if !certs.is_empty() && !wire.carries_certhash() {
        return None;
    }
    Some(DirectShape {
        host,
        port,
        wire,
        certs,
    })
}

/// 把一组地址编码成紧凑形态。
///
/// 认得出的形态走结构化路径并共享去重表，认不出的原样保留。产物经 [`unpack`] **必然**
/// 逐字节还原输入。
pub fn pack(addrs: &[Addr]) -> CompactAddrs {
    let mut out = CompactAddrs::default();
    for addr in addrs {
        let segments: Vec<Protocol<'_>> = addr.as_multiaddr().iter().collect();
        let path =
            compact_path(&segments, &mut out).unwrap_or_else(|| CompactPath::Raw(addr.to_bytes()));
        out.paths.push(path);
    }
    out
}

/// 识别并 intern 一条地址；认不出返回 `None`。
///
/// 识别（纯函数）与 intern（写表）分两步，且**intern 失败要回滚**：中途失败若留下几条没人
/// 引用的表项，它们会照常被序列化进邀请 —— 白白吃掉本模块存在的意义（字节）。
/// 快照两张表的长度、失败时 `truncate` 回去即可：intern 要么复用已有项（不改长度），
/// 要么追加，所以「新加的都在快照之后」恒成立。
fn compact_path(segments: &[Protocol<'_>], out: &mut CompactAddrs) -> Option<CompactPath> {
    let shape = shape_of(segments)?;
    let certhashes_before = out.certhashes.len();
    let relays_before = out.relays.len();
    match intern(shape, out) {
        Some(path) => Some(path),
        None => {
            out.certhashes.truncate(certhashes_before);
            out.relays.truncate(relays_before);
            None
        }
    }
}

/// 识别阶段的产物：形态已确定，但还没有写进任何一张表。
enum Shape {
    Direct(DirectShape),
    Circuit {
        relay: PeerId,
        base: DirectShape,
        hole_punch: bool,
    },
}

/// 纯识别：只看段序，不碰表。
fn shape_of(segments: &[Protocol<'_>]) -> Option<Shape> {
    if let Some(direct) = direct_shape(segments) {
        return Some(Shape::Direct(direct));
    }
    // circuit 形态：`<base>/p2p/<relay>/p2p-circuit[/webrtc]`
    let (hole_punch, segments) = match segments {
        [head @ .., Protocol::WebRTC] => (true, head),
        head => (false, head),
    };
    let [base @ .., Protocol::P2p(relay), Protocol::P2pCircuit] = segments else {
        return None;
    };
    Some(Shape::Circuit {
        relay: *relay,
        base: direct_shape(base)?,
        hole_punch,
    })
}

fn intern(shape: Shape, out: &mut CompactAddrs) -> Option<CompactPath> {
    match shape {
        Shape::Direct(direct) => Some(CompactPath::Direct(intern_direct(direct, out)?)),
        Shape::Circuit {
            relay,
            base,
            hole_punch,
        } => Some(CompactPath::Circuit {
            base: intern_direct(base, out)?,
            relay: CompactAddrs::intern(&mut out.relays, relay.to_bytes())?,
            hole_punch,
        }),
    }
}

fn intern_direct(shape: DirectShape, out: &mut CompactAddrs) -> Option<Direct> {
    let certs = shape
        .certs
        .iter()
        .map(|hash| CompactAddrs::intern(&mut out.certhashes, hash.to_bytes()))
        .collect::<Option<Vec<_>>>()?;
    Some(Direct {
        host: shape.host,
        port: shape.port,
        wire: shape.wire,
        certs,
    })
}

/// 把紧凑形态还原成地址列表。
///
/// 单条路径还原不出来时**跳过它**并继续——地址提示是尽力而为的信息，少一条只是少一条可拨
/// 路径；而整体丢弃会把一份本可用的地址集变成空集，那是最坏的输出形态。这种情况只可能来自
/// 被篡改或损坏的输入：[`pack`] 的产物必然全条可还原。
pub fn unpack(compact: &CompactAddrs) -> Vec<Addr> {
    compact
        .paths
        .iter()
        .filter_map(|path| compact.rebuild(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELAY: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
    const CERT_A: &str = "uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g";
    const CERT_B: &str = "uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA";

    fn addr(value: &str) -> Addr {
        value.parse().expect("测试地址应当可解析")
    }

    /// 判据是**字节**不是 `Display`：同宽不同内容用文本比较有可能漏掉。
    fn assert_roundtrip(addrs: &[Addr]) {
        let restored = unpack(&pack(addrs));
        let expected: Vec<Vec<u8>> = addrs.iter().map(Addr::to_bytes).collect();
        let actual: Vec<Vec<u8>> = restored.iter().map(Addr::to_bytes).collect();
        assert_eq!(
            actual, expected,
            "往返未能逐字节还原：\n  期望 {addrs:?}\n  实得 {restored:?}"
        );
    }

    fn nic(ip: &str) -> Vec<Addr> {
        vec![
            addr(&format!("/ip4/{ip}/tcp/4001")),
            addr(&format!("/ip4/{ip}/udp/4001/quic-v1")),
            addr(&format!(
                "/ip4/{ip}/udp/4004/quic-v1/webtransport/certhash/{CERT_A}/certhash/{CERT_B}"
            )),
            addr(&format!(
                "/ip4/{ip}/udp/4002/webrtc-direct/certhash/{CERT_B}"
            )),
        ]
    }

    fn circuits() -> Vec<Addr> {
        vec![
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{RELAY}/p2p-circuit"
            )),
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{RELAY}/p2p-circuit/webrtc"
            )),
            addr(&format!(
                "/ip4/203.0.113.9/udp/4004/quic-v1/webtransport\
                 /certhash/{CERT_A}/certhash/{CERT_B}/p2p/{RELAY}/p2p-circuit"
            )),
        ]
    }

    /// 已建模的全部形态往返保真——本模块唯一的硬要求。
    #[test]
    fn modelled_shapes_round_trip_byte_for_byte() {
        assert_roundtrip(&[nic("192.168.1.10"), circuits()].concat());
    }

    /// IPv6 与 IPv4 走同一条结构化路径。Tailscale 的 `fd7a:…` 就是这一类。
    #[test]
    fn ipv6_round_trips() {
        assert_roundtrip(&[
            addr("/ip6/fd7a:115c:a1e0::1/tcp/4001"),
            addr(
                "/ip6/2001:db8::1/udp/4004/quic-v1/webtransport/certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g",
            ),
        ]);
    }

    /// 认不出的形态必须原样搬，且往返保真。
    ///
    /// 这条钉的是「新传输不改本模块也能进邀请」：`Raw` 是安全降级，不是失败。
    #[test]
    fn unmodelled_shapes_fall_back_to_raw() {
        let unknown = vec![
            addr("/dns4/relay.example.com/tcp/4001"),
            addr("/ip4/1.2.3.4/udp/4001/quic"),
            addr("/ip4/1.2.3.4/tcp/4001/ws"),
        ];
        let packed = pack(&unknown);
        assert!(
            packed
                .paths
                .iter()
                .all(|p| matches!(p, CompactPath::Raw(_))),
            "未建模形态必须整条落 Raw，不得按建模形态猜：{:?}",
            packed.paths
        );
        assert_roundtrip(&unknown);
    }

    /// 段序不符建模的地址必须落 `Raw`，**不得**被重排成建模形态后编码。
    ///
    /// 重排的后果不是编码失败，而是静默产出一条拨不通的地址。
    #[test]
    fn out_of_order_segments_fall_back_to_raw() {
        // certhash 挂在没有证书的传输上：段序合法可解析，但还原不回去。
        let odd = vec![addr(&format!("/ip4/1.2.3.4/tcp/4001/certhash/{CERT_A}"))];
        let packed = pack(&odd);
        assert!(
            matches!(packed.paths[0], CompactPath::Raw(_)),
            "TCP + certhash 必须落 Raw：{:?}",
            packed.paths
        );
        assert_roundtrip(&odd);
    }

    /// certhash 与 relay 身份**跨地址去重**——本模块存在的理由。
    #[test]
    fn node_level_data_is_stored_once() {
        let packed = pack(&[nic("192.168.1.10"), nic("100.100.200.77"), circuits()].concat());
        assert_eq!(
            packed.certhashes.len(),
            2,
            "两张证书在 5 条地址上出现，表里必须各只有一份：{:?}",
            packed.certhashes.len()
        );
        assert_eq!(
            packed.relays.len(),
            1,
            "3 条 circuit 走同一个 relay，身份必须只存一份"
        );
    }

    /// 紧凑编码必须真的更小——否则本模块没有存在意义。
    ///
    /// 钉的是**数量级**而非精确值（postcard 的 varint 会随端口大小浮动一两个字节）：
    /// 満配那档地址区实测 663 字节，紧凑后应当不到它的一半。
    #[test]
    fn compaction_halves_the_full_house() {
        let addrs = [
            nic("100.100.200.77"),
            nic("192.168.1.10"),
            nic("198.51.100.10"),
            circuits(),
        ]
        .concat();
        let raw: usize = addrs.iter().map(|a| a.to_bytes().len()).sum();
        let packed = postcard::to_stdvec(&pack(&addrs)).expect("postcard 编码不会失败");
        assert!(
            packed.len() * 2 < raw,
            "紧凑编码应当不到原地址区的一半：原 {raw} 字节，紧凑 {} 字节",
            packed.len()
        );
    }

    /// 尾部的 `/p2p/<id>` 段必须完整保留。
    ///
    /// 防的是「由调用方补回被省略的段」那类优化被加回来——它会让同一份编码在不同调用方
    /// 手里还原出不同结果（openspec: design.md Decision 7）。
    #[test]
    fn trailing_identity_segment_is_preserved() {
        assert_roundtrip(&[addr(&format!(
            "/ip4/203.0.113.9/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{RELAY}"
        ))]);
    }

    /// 损坏输入：单条路径还原不出来时只丢那一条，其余照常。
    #[test]
    fn corrupt_path_is_skipped_not_fatal() {
        let good = nic("192.168.1.10");
        let mut packed = pack(&good);
        // 手工把第三条（WebTransport）的 certhash 下标指向表外。
        let CompactPath::Direct(Direct { certs, .. }) = &mut packed.paths[2] else {
            panic!("第三条应当是直连路径");
        };
        certs[0] = 99;

        let restored = unpack(&packed);
        assert_eq!(
            restored.len(),
            good.len() - 1,
            "只应丢掉下标越界的那一条：{restored:?}"
        );
        assert!(
            !restored.iter().any(|a| a.is_webtransport()),
            "丢掉的必须正是被改坏的那条"
        );
    }

    /// **回归钉**：一条 circuit 路径的编码体积必须与嵌套深度无关 —— 也就是说这份 wire
    /// 不能是递归的。
    ///
    /// 递归的代价不是体积而是**可用性**：postcard 反序列化跑在验签之前，一条嵌套上万层的
    /// 邀请文本能让解码栈溢出，而栈溢出是 abort 不是 `Err`，进程直接死。邀请文本又是从
    /// 剪贴板自动读的。判据用「`Circuit` 的 base 只能是 `Direct`」这条类型事实来钉：
    /// 下面这行如果编译不过，说明有人把 `Box<CompactPath>` 加回来了。
    #[test]
    fn circuit_base_cannot_nest() {
        let packed = pack(&circuits());
        let CompactPath::Circuit { base, .. } = &packed.paths[0] else {
            panic!("第一条应当是 circuit");
        };
        // `base: Direct` —— 类型上就不可能再套一层 circuit。这行只是把该事实变成断言。
        let _: &Direct = base;
        assert!(matches!(base.wire, Wire::Tcp));
    }

    /// 空输入不是错误。
    #[test]
    fn empty_input_round_trips() {
        assert_roundtrip(&[]);
    }
}
