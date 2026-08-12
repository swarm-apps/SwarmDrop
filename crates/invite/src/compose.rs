//! 邀请携带哪些地址提示 —— 挑选与回收的**同一份价值序**。
//!
//! 两个方向必须住在一起：[`select_invite_addrs`] 决定「哪些进邀请、按什么顺序」，
//! [`drop_least_valuable_addr`] 决定「码面太密时先丢哪条」。后者的判据全部是对前者桶序的
//! 引用（「桶序是 shared → lan → public，所以从后往前等于……」）—— 分居两个 crate 的话，
//! 那些引用会变成跨 crate 且没有任何东西能校验它们还对不对。
//!
//! 这两件事此前住在 `swarmdrop_core` 的 `pairing::manager`。它们不碰 endpoint、不碰 store、
//! 不碰任何 core 状态，要的只是 [`Addr`] 的分类谓词、[`TransportPolicy`] 与二维码码面
//! —— 三样都在本 crate 或它的依赖里。

use swarmdrop_net_base::Addr;

use crate::TransportPolicy;

/// 每类网络只保留一条原生路径、一条 WebTransport 路径和一条 WebRTC 路径，
/// 避免把全部监听地址复制进邀请。
///
/// 三类路径分别从整类地址中挑选，不能假设同一网卡一定同时监听所有传输；否则先出现的
/// TCP-only 网卡会把后续 WebRTC 地址挤掉，桌面生成的邀请便可能无法从 Web 端打开。
pub fn select_invite_addrs(addrs: Vec<Addr>, policy: TransportPolicy) -> Vec<Addr> {
    let mut shared = Vec::new();
    let mut lan = Vec::new();
    let mut public = Vec::new();
    let mut benchmark = Vec::new();
    let mut circuits = Vec::new();

    // 五个类别互斥（`is_private_lan` 只认 RFC1918，`is_public_routable` 已排除
    // 100.64/10 与 198.18/15），所以分桶顺序只影响可读性，不影响归属。
    for addr in addrs {
        if addr.is_loopback_or_unspecified() {
            continue;
        }
        let bucket = if addr.is_circuit() {
            &mut circuits
        } else if addr.is_shared_address_space() {
            &mut shared
        } else if addr.is_private_lan() {
            &mut lan
        } else if addr.is_public_routable() {
            &mut public
        } else if addr.is_benchmarking_address() {
            &mut benchmark
        } else {
            continue;
        };
        if !bucket.contains(&addr) {
            bucket.push(addr);
        }
    }

    let mut selected = Vec::new();
    if policy == TransportPolicy::LocalOnly {
        append_invite_transports(&mut selected, &lan);
        return selected;
    }

    append_invite_transports(&mut selected, &shared);
    append_invite_transports(&mut selected, &lan);
    append_invite_transports(&mut selected, &public);
    if shared.is_empty() {
        append_invite_transports(&mut selected, &benchmark);
    }
    append_invite_transports(&mut selected, &circuits);
    selected
}

/// native 优先保留可穿过 UDP 封锁的 TCP（无则 QUIC），浏览器另保留 WebTransport 与 WebRTC。
///
/// # 浏览器要两条，不是一条
///
/// 两者**都是**浏览器可拨的直连传输，且互不能替代：
///
/// - WebTransport 快得多（回环 4.5x，见 CLAUDE.md 的实测），但 Safari < 18.2 /
///   Firefox < 114 / 老 WebView **没有这个 API**（`transport.rs` 的
///   `browser_supports_webtransport` 就是为此存在的）。只给它，那些浏览器一条都拨不动。
/// - WebRTC 还独占**打洞**能力（circuit 段之后的 `/webrtc`），WebTransport 至今没有
///   对应机制。
///
/// 所以是追加而不是替换。代价是邀请 wire 变长（每桶约 +85B，两个 certhash 占大头），
/// 由 [`fit_to_scannable`] 按码面密度回收 —— 地址多到扫不动时 WebTransport
/// 是第一批被丢的，见那里的丢弃顺序。
fn append_invite_transports(selected: &mut Vec<Addr>, paths: &[Addr]) {
    // `!is_webrtc()` 不是冗余：WebRTC 地址底下压着 tcp/udp 段（circuit 打洞地址
    // `/ip4/…/tcp/…/p2p-circuit/webrtc` 就是），不排除它就会被当 native 选走，
    // 浏览器那一条反而落空。
    //
    // `!is_webtransport()` 同理且更隐蔽：WebTransport 地址里有 `/quic-v1` 段，
    // `is_quic_v1()` 对它为真（见 `Addr::is_webtransport` 的文档）。不排除的话，
    // 一张没有裸 QUIC 的网卡上 native 会挑走 WebTransport —— 下面那条本来要给它的位置
    // 于是变成重复，白白丢掉一条真正的 native 路径。
    let native = paths
        .iter()
        .find(|addr| addr.is_tcp() && !addr.is_webrtc())
        .or_else(|| {
            paths
                .iter()
                .find(|addr| addr.is_quic_v1() && !addr.is_webrtc() && !addr.is_webtransport())
        });
    let webtransport = paths.iter().find(|addr| addr.is_webtransport());
    let browser = paths.iter().find(|addr| addr.is_webrtc());

    // 三类都没命中说明这一类只有别的传输，留一条免得整类地址丢空。
    let picked = if native.is_none() && webtransport.is_none() && browser.is_none() {
        [paths.first(), None, None]
    } else {
        [native, webtransport, browser]
    };
    for addr in picked.into_iter().flatten() {
        if !selected.contains(addr) {
            selected.push(addr.clone());
        }
    }
}

/// 丢掉一条最不值钱的地址提示，没有可丢的返回 `false`。丢弃顺序见
/// [`fit_to_scannable`]。
pub(crate) fn drop_least_valuable_addr(addrs: &mut Vec<Addr>) -> bool {
    // **最后一条谁都不许动。** 零地址的邀请编得出、扫得动、也复制得走，唯独没有任何
    // 东西可拨 —— 对方拿到它只会静默连不上，两端都不会报错。宁可给一条密度超标的码
    // （用户还能改用粘贴链接），也不给一条形式完好、语义为空的邀请。
    //
    // ⚠️ 这条闸不能只靠下面那个 `!is_circuit()` 兜：`LocalOnly` 邀请**根本没有 circuit
    // 地址**（`select_invite_addrs` 只放私网那一桶），设备名再长一点就会一路裁到空。
    if addrs.len() <= 1 {
        return false;
    }

    // `rposition` = 从后往前。桶序是 shared → lan → public，所以「从后往前」等于
    // 「公网 → 局域网 → 覆盖网」——**而这恰恰把最该留的那条排在了中间**：真正同网的是
    // `lan`（RFC1918），不是 `shared`（100.64/10 覆盖网 / mesh VPN）。而 WebTransport
    // 那 4.5 倍只在同网兑现，所以 lan 那条要留到最后。
    //
    // 这个次序在最常见的密度压力源上就会体现：笔记本挂着 Tailscale 时 shared 桶才有东西，
    // 天真的「从后往前」会先扔掉 lan 的 WebTransport、把覆盖网那条留下来。
    // ⚠️ **三条规则都必须排除 circuit**，不能只让最后一条兜。circuit 基址是从地址簿里
    // 任取一条带传输段的地址拼出来的（`circuit_base` 刻意不做传输白名单），而 bootstrap
    // 就监听 WebTransport —— 于是 `<relay>/…/webtransport/…/p2p-circuit` 是**很常见**的
    // 形态，它同时满足 `is_webtransport()` 与 `!is_private_lan()`，正好命中第一条规则，
    // 结果是「circuit 一条都不丢」这条不变量反过来变成「circuit 最先丢」：邀请编得出、
    // 扫得动，跨网时却零可达路径。
    let victim = addrs
        .iter()
        .rposition(|a| a.is_webtransport() && !a.is_circuit() && !a.is_private_lan())
        .or_else(|| {
            addrs
                .iter()
                .rposition(|a| a.is_webtransport() && !a.is_circuit())
        })
        .or_else(|| addrs.iter().rposition(|a| !a.is_circuit()));
    match victim {
        Some(i) => {
            addrs.remove(i);
            true
        }
        // 只剩 circuit：不丢。跨网时它是唯一可达路径，理由同上面那条下界。
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarmdrop_net_base::SecretKey;

    /// 三端里最小的那块码面（Web 与移动端的白卡内沿）。回归钉按最紧的一档判。
    ///
    /// **传 px 而不是模块数**：模块预算由 `qr::max_modules_for` 从它算，测试里再抄一份
    /// 就又造出了本改动要消灭的那种「必须手工保持一致的常量副本」。
    const SMALLEST_FACE_PX: u32 = 196;

    fn addr(value: &str) -> Addr {
        value.parse().expect("测试地址应当可解析")
    }

    /// 注入用的本机设备名。
    const INJECTED_NAME: &str = "书房 Mac";

    /// 一条 WebTransport 监听地址（两个 certhash = 轮换期两张证书都在场，真机形态）。
    fn webtransport(ip: &str, port: u16) -> String {
        format!(
            "/ip4/{ip}/udp/{port}/quic-v1/webtransport\
             /certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g\
             /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
        )
    }

    #[test]
    fn auto_invite_keeps_bounded_direct_and_relay_paths() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let circuit_tcp = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit");
        let circuit_quic = format!("/ip4/203.0.113.9/udp/4001/quic-v1/p2p/{relay_id}/p2p-circuit");
        let circuit_webrtc = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit/webrtc");
        let shared_wt = webtransport("100.100.200.77", 4004);
        let lan_wt = webtransport("192.168.1.10", 4004);
        let public_wt = webtransport("198.51.100.10", 4004);
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/127.0.0.1/tcp/4001"),
                addr("/ip4/100.100.200.77/tcp/4001"),
                addr("/ip4/100.100.200.77/udp/4001/quic-v1"),
                addr(&shared_wt),
                addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/192.168.1.10/udp/4001/quic-v1"),
                addr(&lan_wt),
                addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
                addr("/ip4/198.51.100.10/tcp/4001"),
                addr("/ip4/198.51.100.10/udp/4001/quic-v1"),
                addr(&public_wt),
                addr("/ip4/198.51.100.10/udp/4002/webrtc-direct"),
                addr("/ip4/198.18.0.1/udp/4001/quic-v1"),
                addr(&circuit_webrtc),
                addr(&circuit_quic),
                addr(&circuit_tcp),
            ],
            TransportPolicy::Auto,
        );
        let expected = vec![
            addr("/ip4/100.100.200.77/tcp/4001"),
            addr(&shared_wt),
            addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
            addr("/ip4/192.168.1.10/tcp/4001"),
            addr(&lan_wt),
            addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
            addr("/ip4/198.51.100.10/tcp/4001"),
            addr(&public_wt),
            addr("/ip4/198.51.100.10/udp/4002/webrtc-direct"),
            addr(&circuit_tcp),
            addr(&circuit_webrtc),
        ];

        assert_eq!(selected, expected);
    }

    /// WebTransport 地址**不得占用 native 那一格**。
    ///
    /// `is_quic_v1()` 对它为真（地址里确有 `/quic-v1` 段），所以一张只有
    /// 「QUIC + WebTransport」的网卡上，漏掉 `!is_webtransport()` 排除条件的实现会让
    /// native 与 webtransport 两格挑到同一条地址 —— 去重之后整个类别只剩一条，
    /// 真正的裸 QUIC 路径被静默丢掉。`assert_eq!` 比对整条清单是抓不到这个的
    /// （上面那条测试里 TCP 总是先命中），所以单列一条。
    #[test]
    fn webtransport_never_takes_the_native_slot() {
        let wt = webtransport("192.168.1.10", 4004);
        let selected = select_invite_addrs(
            vec![addr(&wt), addr("/ip4/192.168.1.10/udp/4001/quic-v1")],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(
            selected,
            vec![addr("/ip4/192.168.1.10/udp/4001/quic-v1"), addr(&wt)],
            "裸 QUIC 与 WebTransport 必须各占一格，且 native 排在前"
        );
    }

    #[test]
    fn auto_invite_uses_benchmark_address_only_without_shared_overlay() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/198.18.0.1/tcp/4001"),
                addr("/ip4/198.18.0.1/udp/4001/quic-v1"),
            ],
            TransportPolicy::Auto,
        );

        assert_eq!(selected, vec![addr("/ip4/198.18.0.1/tcp/4001")]);
    }

    #[test]
    fn local_only_invite_keeps_only_bounded_lan_paths() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/127.0.0.1/tcp/4001"),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/10.0.0.2/udp/4002/webrtc-direct"),
                addr("/ip4/172.16.0.3/tcp/4001"),
                addr("/ip4/198.51.100.10/tcp/4001"),
            ],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(
            selected,
            vec![
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/10.0.0.2/udp/4002/webrtc-direct"),
            ]
        );
    }

    /// 真实网络配置下的 QR 密度回归钉。
    ///
    /// 判据是**扫得动**，不是「编得出」—— 容量从来不是约束（ECL::M 下 2079 字节才到顶），
    /// 先出事的永远是 px/模块（见 `qr::MIN_PX_PER_MODULE`）。
    ///
    /// ⚠️ **这条测试是加 WebTransport 时补的，而它一补上就发现改动前也已经卡线**：
    /// 满配桌面（CGNAT 覆盖网 + 局域网 + 公网直连 + 中继）当时是 97 模块，距上限 98
    /// 只剩一格。所以裁剪不是给 WebTransport 擦屁股的补丁，是这个函数一直缺的那道闸。
    ///
    /// 实测模块数（含 quiet zone，上限 98）。wire v2 的紧凑地址编码
    /// （`swarmdrop_net_base::compact`）把地址区从 663 压到约 252 字节：
    ///
    /// | 配置 | wire v1 不裁 | wire v1 裁后 | **wire v2** |
    /// |---|---:|---:|---|
    /// | 家用 lan + circuit | 93 | 93 | **85**（6 条全留） |
    /// | 公网 lan + public + circuit | 105 | 97 | **89**（9 条全留） |
    /// | CGNAT shared + lan + circuit | 105 | 97 | **89**（9 条全留） |
    /// | 满配 四类齐全 | 117 | 97（11 → 8） | **93**（12 条全留） |
    /// | 满配 + 40 字中文设备名 | — | — | **97**（12 → 8，裁了 4 条） |
    ///
    /// 也就是说**常规设备名下裁剪已经完全不触发**，四档全留。
    ///
    /// ⚠️ **但最坏情况的主导变量不再是地址数，而是设备名。** `DeviceName::MAX_CHARS = 40`，
    /// 40 个中文字是 120 字节 —— 比压缩后的整个地址区（约 252 字节）的一半还多。最后一档
    /// 若不裁是 **101 模块**，超出上限 3 格，于是又回到「裁掉 4 条」的老形态。
    ///
    /// 这条尤其要记住：它意味着「压缩解决了密度问题」是**半句话**。真要让裁剪归零，
    /// 还得让预算跟上生成端自己的码面（桌面实为 240px = 120 模块，101 放得下），
    /// 见 `the_budget_decides_whether_anything_is_trimmed`。
    /// **同一条邀请，两个预算，两种结果** —— 「预算由 UI 传入」的全部意义就在这条上。
    ///
    /// 満配 + 顶格设备名不裁是 101 模块：三端最小码面（196px → 98）放不下，而桌面自己的
    /// 240px（→ 120）放得下。此前 core 里一个常量按最小码面一刀切，桌面那 22 格余量白白
    /// 浪费；现在各端传自己的数。
    ///
    /// 顺带钉住第二件事：**链接不裁**。密度是二维码的约束，粘贴那条路径没有这回事。
    #[test]
    fn the_budget_decides_whether_anything_is_trimmed() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let nic = |ip: &str| {
            vec![
                addr(&format!("/ip4/{ip}/tcp/4001")),
                addr(&webtransport(ip, 4004)),
                addr(&format!(
                    "/ip4/{ip}/udp/4002/webrtc-direct\
                     /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
                )),
            ]
        };
        let dialable = [
            nic("100.100.200.77"),
            nic("192.168.1.10"),
            nic("198.51.100.10"),
            vec![addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit"
            ))],
        ]
        .concat();

        let secret = SecretKey::generate();
        let invite = crate::PairInvite::generate(
            &secret,
            select_invite_addrs(dialable, TransportPolicy::Auto),
            TransportPolicy::Auto,
            // 顶格设备名：`swarmdrop_host` 的 `DeviceName::MAX_CHARS = 40`，中文 3 字节/字。
            // 这才是最坏情况的主导变量 —— 120 字节比压缩后的整个地址区一半还多。
            "字".repeat(40),
            "macos".into(),
            1_700_000_000,
        );
        // 链接：完整地址集，不经任何裁剪。
        let link = invite.sign(&secret).encode();
        let link_addrs = crate::PairInvite::decode(&link)
            .unwrap()
            .inviter
            .addrs
            .len();

        let count = |face_px: u32| {
            crate::PairInvite::decode(
                &crate::qr::fit_to_scannable(&link, face_px)
                    .expect("裁剪后仍须验签通过")
                    .0,
            )
            .expect("裁剪后仍须验签通过")
            .inviter
            .addrs
            .len()
        };
        // 196px 的码面（Web 与移动端）：放不下，得裁。
        assert!(
            count(196) < link_addrs,
            "196px 码面放不下这条邀请，本该裁地址"
        );
        // 240px 的码面（桌面）：一条都不用裁。
        assert_eq!(
            count(240),
            link_addrs,
            "240px 码面放得下全部 {link_addrs} 条地址，不该裁"
        );
    }

    #[test]
    fn invite_stays_scannable_at_every_scale() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        // 一张网卡监听全部四种传输 —— 真机上每类地址空间都长这样。
        // （`//` 不是 `///`：doc comment 挂不到 `let` 上，rustc 会报 `unused_doc_comments`。）
        let nic = |ip: &str| {
            vec![
                addr(&format!("/ip4/{ip}/tcp/4001")),
                addr(&format!("/ip4/{ip}/udp/4001/quic-v1")),
                addr(&webtransport(ip, 4004)),
                addr(&format!(
                    "/ip4/{ip}/udp/4002/webrtc-direct\
                     /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
                )),
            ]
        };
        // ⚠️ **第三条必须是 WebTransport 基址的 circuit。** circuit 基址取自地址簿里任一
        // 带传输段的地址（`circuit_base` 刻意不做白名单），而 bootstrap 本身就监听
        // WebTransport —— 这个形态在真机上很常见，且它同时满足 `is_webtransport()` 与
        // `!is_private_lan()`。裁剪规则若不显式排除 circuit，它会被**最先**丢掉，
        // 而只用 TCP circuit 的测试永远抓不到（那是这条回归躲过第一版审查的原因）。
        let circuits = vec![
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit"
            )),
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit/webrtc"
            )),
            addr(&format!(
                "{}/p2p/{relay_id}/p2p-circuit",
                webtransport("203.0.113.9", 4004)
            )),
        ];
        let full_house = [
            nic("100.100.200.77"),
            nic("192.168.1.10"),
            nic("198.51.100.10"),
            circuits.clone(),
        ]
        .concat();
        // 由少到多。第三项是**期望裁剪是否触发** —— wire v2 之后常规设备名下四档全是 false，
        // 只有最后那档「満配 + 顶格设备名」才回到裁剪路径。
        //
        // ⚠️ 最后一档不是凑数：`DeviceName::MAX_CHARS = 40`，40 个中文字 = 120 字节，比压缩
        // 后的整个地址区一半还多。少了它，这条测试会让人以为「压缩之后不用裁了」——而那句话
        // 只对短名成立。
        let scenarios: Vec<(&str, Vec<Addr>, String, bool)> = vec![
            (
                "家用 lan+circuit",
                [nic("192.168.1.10"), circuits.clone()].concat(),
                INJECTED_NAME.into(),
                false,
            ),
            (
                "公网 lan+public+circuit",
                [nic("192.168.1.10"), nic("198.51.100.10"), circuits.clone()].concat(),
                INJECTED_NAME.into(),
                false,
            ),
            (
                "CGNAT shared+lan+circuit",
                [nic("100.100.200.77"), nic("192.168.1.10"), circuits.clone()].concat(),
                INJECTED_NAME.into(),
                false,
            ),
            (
                "满配 shared+lan+public+circuit",
                full_house.clone(),
                INJECTED_NAME.into(),
                false,
            ),
            (
                "满配 + 40 字中文设备名",
                full_house,
                "字".repeat(40), // = swarmdrop_host 的 `DeviceName::MAX_CHARS`；本 crate 不依赖 host，
                // 所以只能写死。那边改了这里要跟着改，否则这一档不再是顶格。
                true,
            ),
        ];

        for (name, dialable, display_name, expect_trim) in scenarios {
            let selected = select_invite_addrs(dialable, TransportPolicy::Auto);
            let before = selected.len();
            let circuits_before = selected.iter().filter(|a| a.is_circuit()).count();
            let secret = SecretKey::generate();
            let invite = crate::PairInvite {
                // 固定 capability：`generate` 那份是随机的，而 base32 payload 里数字连段的
                // 长短会改变最优分段的取舍，码面因此浮动一档（qr.rs 的 `fixed_invite` 记着
                // 同一件事）。回归钉要可复现。
                capability: [
                    0x3f, 0x1c, 0xa7, 0x02, 0x9b, 0x64, 0xd8, 0x51, 0x0e, 0xf3, 0x27, 0xbc, 0x45,
                    0x8a, 0x16, 0xd0,
                ],
                inviter: swarmdrop_net_base::NodeAddr::with_addrs(secret.node_id(), selected),
                expires_at: 1_700_086_400,
                transport_policy: TransportPolicy::Auto,
                display_name,
                display_platform: "macos".into(),
            };
            let (encoded, modules) =
                crate::qr::fit_to_scannable(&invite.sign(&secret).encode(), SMALLEST_FACE_PX)
                    .expect("裁剪后必须仍编得出码");
            assert!(
                modules <= crate::qr::max_modules_for(SMALLEST_FACE_PX),
                "{name}：码面 {modules} 模块超过 {SMALLEST_FACE_PX}px 下可扫的上限\
                 （邀请串 {} 字符）",
                encoded.len()
            );

            // 裁剪的是地址，解码回来才能看清它到底留下了什么。
            let decoded = crate::PairInvite::decode(&encoded).expect("自签邀请可解码");
            // **数量断言，不是 `any`。** `any(is_circuit())` 只要还剩一条就绿，于是
            // 「裁掉了 WebTransport 基址那条 circuit」这类回归它一个都抓不到 —— 而
            // 「一条都不许裁」本来就是按条数说的。
            let circuits_after = decoded
                .inviter
                .addrs
                .iter()
                .filter(|a| a.is_circuit())
                .count();
            assert_eq!(
                circuits_after, circuits_before,
                "{name}：circuit 是跨网唯一可达路径，一条都不许裁\
                 （{circuits_before} → {circuits_after}）"
            );
            if expect_trim {
                // 断言闸**确实动了手**。少了这条，一个恒等于「不裁」的实现也能让上面的
                // 码面断言通过 —— 因为它本来就压着上限。
                assert!(
                    decoded.inviter.addrs.len() < before,
                    "{name}：码面本该超标，裁剪却一条没动（{before} 条原样留下）"
                );
            } else {
                // 这几档必须**一条都不裁**。断言「全留」而不是「留住 WebTransport」：
                // 后者对「裁掉了别的什么」没有区分力，而 wire v2 之后这几档本来就该零裁剪。
                assert_eq!(
                    decoded.inviter.addrs.len(),
                    before,
                    "{name}：这一档放得下全部 {before} 条地址，不该裁掉任何一条"
                );
            }
        }
    }

    /// **零地址的邀请是最坏的输出形态**，裁剪必须留住最后一条。
    ///
    /// `LocalOnly` 邀请根本没有 circuit 地址（只放私网那一桶），所以「circuit 不丢」这条
    /// 规则对它一点保护都没有 —— 少了下界，设备名长一点就会一路裁到空，而那样的邀请
    /// **编得出、扫得动、复制得走，唯独没有任何东西可拨**，两端都不会报错。
    #[test]
    fn trimming_never_empties_the_address_hints() {
        let mut addrs = vec![
            addr("/ip4/192.168.1.10/tcp/4001"),
            addr(&webtransport("192.168.1.10", 4004)),
        ];
        assert!(drop_least_valuable_addr(&mut addrs), "两条时该丢一条");
        assert_eq!(addrs.len(), 1);
        assert!(
            !drop_least_valuable_addr(&mut addrs),
            "只剩一条时必须停手，哪怕它不是 circuit"
        );
        assert_eq!(addrs.len(), 1, "最后一条不许丢");
    }

    /// **WebTransport 基址的 circuit 不是「一条 WebTransport」，它是 circuit。**
    ///
    /// circuit 基址取自地址簿里任一带传输段的地址（`circuit_base` 刻意不做传输白名单），
    /// 而 bootstrap 本身就监听 WebTransport —— 所以
    /// `<relay>/…/webtransport/…/p2p-circuit` 是真机上很常见的形态。它同时满足
    /// `is_webtransport()` 与 `!is_private_lan()`，若裁剪的前两条规则不显式排除 circuit，
    /// 它会被**最先**丢掉：「circuit 一条都不丢」这条不变量反过来变成「circuit 最先丢」，
    /// 而邀请照样编得出、扫得动，只是跨网时零可达路径。
    ///
    /// 走整条编码链路的 `invite_stays_scannable_at_every_scale` 抓不到它——那里还有别的
    /// circuit 顶着，条数断言才刚补上。这一条直接打在裁剪函数上，形态最小、失败最明确。
    #[test]
    fn trimming_never_sacrifices_a_webtransport_based_circuit() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let mut addrs = vec![
            // 公网直连的 WebTransport：该它先走。
            addr(&webtransport("198.51.100.10", 4004)),
            // 经中继的 WebTransport circuit：跨网唯一可达路径，一条都不许动。
            addr(&format!(
                "{}/p2p/{relay_id}/p2p-circuit",
                webtransport("203.0.113.9", 4004)
            )),
        ];

        assert!(drop_least_valuable_addr(&mut addrs), "两条时该丢一条");
        assert_eq!(addrs.len(), 1);
        assert!(
            addrs[0].is_circuit(),
            "留下的必须是 circuit —— 丢掉它等于把跨网唯一可达路径先丢了"
        );
    }

    /// 同网那条 WebTransport 留到最后 —— 它那 4.5 倍只在 RFC1918 上兑现。
    ///
    /// 这条钉的是**次序**而不是「丢了几条」：桶序是 shared → lan → public，天真的
    /// 「从后往前」会得到 public → **lan** → shared，把最该留的那条排在中间。
    /// 而挂着 Tailscale 的笔记本（shared 桶有东西）正是密度压力最常见的来源。
    #[test]
    fn webtransport_in_the_lan_bucket_is_dropped_last() {
        let shared_wt = webtransport("100.100.200.77", 4004);
        let lan_wt = webtransport("192.168.1.10", 4004);
        let public_wt = webtransport("198.51.100.10", 4004);
        let mut addrs = select_invite_addrs(
            vec![
                addr("/ip4/100.100.200.77/tcp/4001"),
                addr(&shared_wt),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr(&lan_wt),
                addr("/ip4/198.51.100.10/tcp/4001"),
                addr(&public_wt),
            ],
            TransportPolicy::Auto,
        );

        let mut dropped = Vec::new();
        for _ in 0..3 {
            let before = addrs.clone();
            assert!(drop_least_valuable_addr(&mut addrs));
            dropped.push(
                before
                    .into_iter()
                    .find(|a| !addrs.contains(a))
                    .expect("每轮恰好少一条"),
            );
        }

        assert_eq!(
            dropped,
            vec![addr(&public_wt), addr(&shared_wt), addr(&lan_wt)],
            "WebTransport 的丢弃次序必须是 公网 → 覆盖网 → 局域网"
        );
    }

    /// 兜底分支：某一类只剩本函数不认识的传输时，仍要留一条，不能把整类丢空。
    /// 当前 transport 栈（TCP / QUIC / WebTransport / WebRTC）走不到这里，
    /// 它是新增传输时的安全网。
    #[test]
    fn unknown_transport_class_still_keeps_one_path() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/192.168.1.10/udp/4001"),
                addr("/ip4/192.168.1.11/udp/4002"),
            ],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(selected, vec![addr("/ip4/192.168.1.10/udp/4001")]);
    }
}
