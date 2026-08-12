//! 邀请二维码生成（三端统一规范，wasm-clean）。
//!
//! 三端渲染的唯一编码源——把 QR 的编码策略单点固化，避免各端 JS 库各写一遍导致漂移
//! （openspec: pair-invite-ui design D1/D2）。三端拿本模块产出的 SVG 字符串（web 直接
//! `innerHTML`、桌面 `dangerouslySetInnerHTML`）或模块矩阵（RN 用 react-native-svg 自绘）。
//!
//! 关键规范（全部固化在此，调用方传 canonical 邀请链接即可）：
//! - **原样编码，不做大小写归一**。canonical 是「小写 scheme/host/path + 大写 base32
//!   payload」，最优分段会把小写前缀那 23 个字符单独编成一个 byte 段，payload 那两百多个
//!   字符仍走 alphanumeric —— 实测码面与「整串大写」**同一 version**（确定性样本下都是 61×61），
//!   所以没有任何理由为了二维码去牺牲链接的正常外观。
//!   （历史：fast_qr 时代整串统一 mode，必须全大写才能进 alphanumeric，canonical 因此一度
//!   长成 `HTTPS://SWARMAPP.CN/P/#…`。换成支持最优分段的编码器后这个约束消失了。）
//! - **最优分段**（`QrCode::new` → `bits::encode_auto`）同时吸收那个 `#`（它不在
//!   alphanumeric 字符集内）与小写前缀，而不是把整串拖进 byte mode（+30% 面积）。
//!   见 `optimal_segmentation_beats_byte_mode`。
//! - **ECL::M**（15% 纠错）：屏→摄像头近距干净场景足够；Q/H 只会顶高版本更难扫，且无 logo。
//!   这也是 `QrCode::new` 的默认级别。
//! - **quiet zone 4 模块**（ISO 硬性要求）。
//! - 配色由渲染端负责：**深模块 + 白底，不随暗色主题反色**（摄像头对反色 QR 识别差）。

use qrcode::QrCode;

use crate::invite::SignedInvite;

/// 两侧各 4 模块的 quiet zone（ISO 硬性要求）。
const QUIET_ZONE: usize = 4;

/// 摄像头能把一个模块读出来所需的最低像素密度。
///
/// **这是本 crate 唯一的密度常量，不再有跨语言副本。** 曾经的形态是 core 里一个
/// `INVITE_QR_MAX_MODULES = 98`（三端最小码面 196px ÷ 2 反推），而三端 UI 各自的码面尺寸
/// 是另外三份「必须与它保持一致」的数字，没有任何门禁把四者钉在一起 —— 那条漂移**真的
/// 发生过**：注释里写着「桌面 260px」，实际传的是 240。
///
/// 现在各端把自己的码面 px 传进来（那是它唯一知道、且本来就定义在那里的数），
/// 由 [`max_modules_for`] 换算成模块预算。要「保持一致」的东西不存在了。
pub const MIN_PX_PER_MODULE: u32 = 2;

/// 码面下限：QR version 1（21 模块）加两侧 quiet zone，按 [`MIN_PX_PER_MODULE`] 折成 px。
///
/// 低于它，**任何内容**都不可能扫得出来，所以那只会是调用方把数传错了 —— 传了 0、
/// 传了负数被截断成巨大的 u32、或者把白卡内边距减重了。给这种输入静默降级（一路裁到只剩
/// 一条地址、再交出一张照样扫不动的码）是最难归因的失败形态，宁可当场报错。
pub const MIN_FACE_PX: u32 = (21 + QUIET_ZONE as u32 * 2) * MIN_PX_PER_MODULE;

/// 给定码面边长（px），能容纳的最大模块数（含 quiet zone）。
///
/// 传的必须是**二维码实际占据的那块区域**，不是外框 —— 三端的白卡内边距各不相同
/// （桌面 padding 在外框上、移动端在内），这是搬这个数时最容易搞错的一步。
pub const fn max_modules_for(face_px: u32) -> usize {
    (face_px / MIN_PX_PER_MODULE) as usize
}

/// 三端统一的 QR 编码：canonical 链接原样 → 最优分段 + ECL::M。
/// **编码策略单点**——SVG / matrix 两个渲染出口都经此，改 ECL/模式只此一处。
fn build_qr(invite: &str) -> Result<QrCode, QrError> {
    // 不做大小写归一：最优分段自己会把小写前缀与大写 payload 分成两段，
    // 码面与整串大写相同（见模块文档与 `qr_size_baseline`）。
    QrCode::new(invite.as_bytes()).map_err(|e| QrError(e.to_string()))
}

/// 把邀请裁到 `face_px` 这块码面上扫得动，返回**最终的串与码**。
///
/// **裁剪发生在渲染侧，是刻意的。** 密度是二维码的约束，链接没有这回事——生成侧一刀切地裁，
/// 等于让复制粘贴那条路径为扫码受委屈。现在链接保留全部地址，二维码按自己的码面各裁各的，
/// 响应式码面也天然正确（换个尺寸重渲染即可，不必重新生成邀请）。
///
/// 能这么做的前提是**地址提示不在签名覆盖范围内**（wire v2），所以渲染侧不持有私钥也能裁。
///
/// **同时返回串与码**，而不是只返回串再让调用方编一次：判据本来就是「把它编出来看多少模块」，
/// 循环最后一轮已经编好了。只回串等于每次渲染都白编一遍整张码。
///
/// **本函数住在 `qr` 而不是 `compose`**：它的循环判据是码面，属于二维码的事；`compose` 只
/// 提供「先丢哪条」的价值序。反过来放会让两个模块互相调用，谁都不能单独读。
///
/// # 为什么必须有这一步
///
/// 地址是「每桶挑几条 × 桶数」，而**桶数没有上界**：一台同时有 CGNAT 覆盖网、局域网、
/// 公网直连的机器就是 3 个桶。实测（`invite_stays_scannable_at_every_scale`）加 WebTransport
/// 之前，满配桌面的码面已经是 97 模块 —— 距上限只剩 1 格。也就是说这里在改动之前就没有
/// 余量了，**再往邀请里加任何东西都会静默越线**：QR 照样生成、链接照样能用，只有真机扫码
/// 那一下失败，而那是最难归因的失败形态。
///
/// # 丢弃顺序 = 价值的反序
///
/// 1. **WebTransport** —— 纯增益。丢掉后浏览器仍能靠同桶的 webrtc-direct 拨通；配对一旦
///    成功，identify 会把完整监听地址交回来，后续传输照样走 WebTransport。代价只是首次
///    拨号慢一点，不是连不上。同类里**私网（`lan`）那条留到最后**，其余从后往前丢 ——
///    那 4.5 倍只在真正同网时兑现，而同网指的是 RFC1918，不是 100.64/10 覆盖网。
/// 2. **其余直连地址**，同样从后往前。
/// 3. **circuit 一条都不丢**，**最后一条地址也一条都不丢**（哪怕它不是 circuit）。
///    前者是因为跨网时 circuit 是唯一可达路径，而扫码方在哪个网络、生成邀请的这一端
///    并不知道；后者是因为零地址的邀请**一切正常只是拨不动**，是这里最坏的输出形态。
///
/// 于是这个函数有一个**下界而非终点**：它保证「裁到扫得动」是尽力而为，不保证一定做到。
/// 真裁不下去时返回的是一条密度超标但语义完好的邀请 —— 用户还能改用粘贴链接。
///
/// 逐次重编码而不是估算字节：判据就是最终码面本身，零推导误差。地址至多十来条，
/// 生成邀请又是低频动作，这点开销无关紧要。
///
/// **本函数拿不到私钥。** 地址提示不在签名覆盖范围内（wire v2，见 [`InviteV2`](crate::invite)），
/// 所以裁剪是纯结构操作：解出 [`SignedInvite`]、删几条、重新编码，签名一字未动。这不只是
/// 省下每轮一次 ed25519 —— 它让「循环里顺带重签名」在类型上无法发生，也让裁剪可以发生在
/// **渲染侧**（二维码按自己的码面各裁各的，链接则保留全部地址）。
fn fit(invite: &str, face_px: u32) -> Result<(String, QrCode), QrError> {
    // 码面小到连 QR version 1（21 模块 + quiet zone）都摆不下时，任何内容都扫不出来 ——
    // 那不是「地址太多」而是调用方把数传错了（传了 0、传了负数被截断、或把内边距减重了）。
    // 此时静默裁到只剩一条地址、给出一张照样扫不动的码，是最难归因的失败形态。
    if face_px < MIN_FACE_PX {
        return Err(QrError(format!(
            "码面 {face_px}px 太小（至少 {MIN_FACE_PX}px）——检查传的是不是二维码本体的边长"
        )));
    }
    let budget = max_modules_for(face_px);
    let mut signed = SignedInvite::decode(invite)
        // 解不开就报错，**不能**原样渲染：那样密度预算静默失效，调用方还分不清
        // 「我没看懂这条邀请」和「我裁好了」。
        .map_err(|e| QrError(format!("不是一条有效的邀请: {e}")))?;
    loop {
        let encoded = signed.encode();
        match build_qr(&encoded) {
            Ok(qr) if qr.width() + QUIET_ZONE * 2 <= budget => return Ok((encoded, qr)),
            // 编不出码（真的超了 QR 容量）与「码面太密」在这里是同一件事：都得再丢一条。
            attempt => {
                if !crate::compose::drop_least_valuable_addr(signed.addrs_mut()) {
                    return attempt.map(|qr| (encoded, qr));
                }
            }
        }
    }
}

/// 裁到 `face_px` 这块码面放得下，返回**裁剪后的邀请串与它的模块数**。
///
/// 渲染路径不走这里（[`fit`] 直接把码交出去，省一次编码）；这个出口给需要看「裁了什么」
/// 的调用方 —— 目前只有 `compose` 的回归钉。
#[cfg(test)]
pub(crate) fn fit_to_scannable(invite: &str, face_px: u32) -> Result<(String, usize), QrError> {
    let (encoded, qr) = fit(invite, face_px)?;
    let modules = qr.width() + QUIET_ZONE * 2;
    Ok((encoded, modules))
}

/// 生成邀请二维码的 SVG 字符串（深模块 `#0a0a0a`，透明背景——渲染端套白卡）。
///
/// `invite` 传 canonical 邀请链接（`PairInvite::encode` 的产物），`face_px` 传二维码实际
/// 占据的边长（见 [`max_modules_for`]）。
pub fn invite_qr_svg(invite: &str, face_px: u32) -> Result<String, QrError> {
    use qrcode::render::svg;

    let (_, qr) = fit(invite, face_px)?;
    Ok(qr
        .render()
        .quiet_zone(true)
        .dark_color(svg::Color("#0a0a0a"))
        .light_color(svg::Color("transparent"))
        .build())
}

/// 生成邀请二维码的模块矩阵（`true` = 深模块）。RN 端按此自绘 `<Rect>`；
/// 已含 4 模块 quiet zone 边距。
pub fn invite_qr_matrix(invite: &str, face_px: u32) -> Result<Vec<Vec<bool>>, QrError> {
    let (_, qr) = fit(invite, face_px)?;
    let size = qr.width();
    let full = size + QUIET_ZONE * 2;
    let mut matrix = vec![vec![false; full]; full];
    for (r, row) in qr.to_colors().chunks(size).take(size).enumerate() {
        for (c, module) in row.iter().enumerate() {
            matrix[r + QUIET_ZONE][c + QUIET_ZONE] = *module == qrcode::Color::Dark;
        }
    }
    Ok(matrix)
}

/// 二维码生成失败（邀请串过长超出 QR 容量等）。
#[derive(Debug, thiserror::Error)]
#[error("二维码生成失败: {0}")]
pub struct QrError(String);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PairInvite, TransportPolicy};

    /// 桌面码面。取偏大的一档，免得 fit 在无关的测试里插手。
    const TEST_FACE_PX: u32 = 240;
    use swarmdrop_net_base::SecretKey;

    fn sample_invite() -> String {
        let sk = SecretKey::generate();
        PairInvite::generate(
            &sk,
            vec!["/ip4/192.168.1.10/tcp/4001".parse().unwrap()],
            TransportPolicy::Auto,
            "书房 Mac".into(),
            "macos".into(),
            1_700_000_000,
        )
        .encode(&sk)
    }

    #[test]
    fn svg_generates_and_contains_paths() {
        let svg = invite_qr_svg(&sample_invite(), TEST_FACE_PX).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("path") || svg.contains("rect"));
    }

    #[test]
    fn matrix_is_square_with_quiet_zone() {
        let m = invite_qr_matrix(&sample_invite(), TEST_FACE_PX).unwrap();
        assert!(!m.is_empty());
        assert!(m.iter().all(|row| row.len() == m.len()), "矩阵必须是正方形");
        // 四角属 quiet zone，必为浅模块
        assert!(!m[0][0] && !m[0][m.len() - 1]);
    }

    /// **回归钉子**：最优分段必须真的在起作用。
    ///
    /// canonical（小写前缀 + 大写 base32 payload）会被分成 byte + alphanumeric 两段；
    /// 把 payload 也变成小写后整串只能走 byte mode。前者必须更小。
    ///
    /// 这条红了意味着编码器换回了「全串统一 mode」的实现（fast_qr 那类），
    /// 二维码会明显变密、扫码成功率下降。
    #[test]
    fn optimal_segmentation_beats_byte_mode() {
        let invite = sample_invite();
        // 经 build_qr 而非直接 QrCode::new：否则这条只是「依赖库行为」的金丝雀，
        // 抓不到 build_qr 自身的任何改动。
        let segmented = build_qr(&invite).unwrap();
        let all_byte = QrCode::new(invite.to_lowercase().as_bytes()).unwrap();
        assert!(
            segmented.width() < all_byte.width(),
            "最优分段应比全 byte mode 版本更低: segmented={} all_byte={}",
            segmented.width(),
            all_byte.width()
        );
    }

    /// **不做大小写归一**这个决定的真实理由是「链接可用性」，不是「大写没收益」。
    ///
    /// 曾经的理由是后者：fast_qr 只有全串统一 mode，canonical 因此一度长成
    /// `HTTPS://SWARMAPP.CN/P/#…`；换成支持最优分段的编码器后实测两者码面相同，于是改回
    /// 小写。**但前缀换成 GitHub Pages 子路径（42 字符）后这个等式不再总成立** —— 实测某些
    /// 随机 payload 下大写确实能省一个 version（61 → 57），因为长前缀整体进 alphanumeric
    /// 省下的位数跨过了边界。
    ///
    /// 即便如此也不能大写：`/SwarmDrop/` 是仓库名，**GitHub Pages 路径区分大小写**，
    /// 大写成 `/SWARMDROP/` 直接 404 —— 扫码跳到一个打不开的落地页。粘贴路径因为前缀匹配
    /// 大小写不敏感仍能配对，所以这个回归只在真机扫码时暴露。
    ///
    /// 所以这里只钉一个方向：大写**不会更大**。真正防「有人把 `to_ascii_uppercase()` 加回
    /// `build_qr`」的是 [`build_qr_encodes_input_verbatim`]（用模块矩阵，宽度没有区分力）。
    #[test]
    fn uppercasing_never_grows_the_code() {
        let invite = fixed_invite();
        let original = build_qr(&invite).unwrap().width();
        let upper = build_qr(&invite.to_ascii_uppercase()).unwrap().width();
        assert!(
            upper <= original,
            "大写不应让码面更大（原样 {original}，大写 {upper}）"
        );
    }

    /// **`build_qr` 必须编码传入的串本身，不做任何大小写归一。**
    ///
    /// 这条钉的不是码面而是**链接可用性**：确定性样本下大写与原样码面相同（都 61），
    /// 所以 `qr_size_baseline` 的 `width == 61` 对「有人把 `to_ascii_uppercase()` 加回
    /// `build_qr`」**完全没有区分力** —— 加回去照样绿。
    ///
    /// 而后果是真的：加回大写化后路径段变成 `/SWARMDROP/P/`，而 GitHub Pages 区分大小写
    /// （`/SwarmDrop/` 是仓库名、`p/` 是小写目录）→ **扫码跳网页 404**。粘贴路径因前缀
    /// 匹配大小写不敏感仍正常，所以这个回归只在真机扫码时暴露。
    ///
    /// 判据用模块矩阵而非宽度：同宽但内容不同，矩阵能区分，宽度不能。
    #[test]
    fn build_qr_encodes_input_verbatim() {
        let invite = sample_invite();
        assert_ne!(
            build_qr(&invite).unwrap().to_colors(),
            build_qr(&invite.to_ascii_uppercase()).unwrap().to_colors(),
            "build_qr 似乎对输入做了大小写归一——那会让扫码跳到一个 404 的大写路径"
        );
    }

    /// 码面尺寸基线（密度退化哨兵）。
    ///
    /// 确定性样本实测：邀请串 294 字符 → 最优分段 **61×61**（QR version 11，`width = 4v+17`）。
    ///
    /// ⚠️ **这个基线比 wire v1 时代更大（287/57 → 294/61），而那不是退步。** 样本只带 1 条
    /// 地址提示，而 v2 的紧凑编码有约 5 字节的固定开销（core 的长度前缀 + certhash/relay
    /// 两张空表 + 路径变体 tag）。收益在地址多的时候兑现：真实的満配桌面地址区
    /// **663 → 约 252 字节**。拿単地址样本判断 v2 划不划算会得出完全相反的结论——
    /// 那是 `swarmdrop_core` 的 `invite_stays_scannable_at_every_scale` 存在的意义。
    ///
    /// ⚠️ **这是哨兵，不是「生产码面就是 61」的承诺。** 真实邀请的宽度**随内容浮动**：
    /// 随机 base32 payload 里的 `2-7` 属于 QR 的 numeric 类，数字连段的长短会改变最优分段
    /// 的取舍，而 42 字符的前缀让码面正好卡在 version 边界上。实测同一份 wire 的随机样本
    /// 落在 **57 ~ 65** 之间（纯字母 57、纯数字 49、周期性混排 65、真实随机多数 61）。
    /// 所以这条测试用固定私钥 + 固定 capability 的 [`fixed_invite`]，否则它会随机红绿
    /// —— 那正是本次改动暴露出来的既有缺陷。
    ///
    /// 这条红了说明码面确实变了——wire 加了字段、canonical 前缀变长、或编码规范退化。
    /// 不要随手改数字，先确认变化是有意的：码面变大直接影响扫码成功率。
    ///
    /// ⚠️ 钉的是**最小形态**：`sample_invite()` 只带 1 个地址提示。真机上桌面通常有多个
    /// 监听地址，每多约 15 个 base32 字符就逼近下一个 version 边界，所以生产码面会明显
    /// 大于这个基线。
    /// 固定私钥（protobuf 编码，`SecretKey::from_protobuf` 的入参格式）。
    ///
    /// 码面基线必须**确定**才有意义，而 `sample_invite()` 每次随机生成 —— 实测同一条测试
    /// 在不同运行里给出 57 / 61 两种宽度，因为 base32 payload 里的 `2-7` 属于 QR 的
    /// numeric 类（3.33 bit/字符），随机内容里数字连段的长短会改变最优分段的取舍，而前缀
    /// 变长后码面正好卡在 version 边界上。ed25519 签名是确定性的（RFC 8032），所以固定
    /// 私钥 + 固定字段 ⇒ 整串字节可复现 ⇒ 宽度可复现。
    const FIXED_KEY_PROTOBUF: &[u8] = &[
        0x08, 0x01, 0x12, 0x40, 0x97, 0x59, 0x30, 0x96, 0xf4, 0x3c, 0xf8, 0x8a, 0x0d, 0xd2, 0x55,
        0x6e, 0xe7, 0xb7, 0xc6, 0xcf, 0x8c, 0x41, 0xc7, 0x5a, 0xae, 0x02, 0xe0, 0xe1, 0xaa, 0x66,
        0x52, 0xd6, 0x2c, 0x2b, 0x89, 0xd3, 0xf0, 0xda, 0x62, 0x87, 0xc1, 0x2e, 0x09, 0x84, 0xb4,
        0x6c, 0xfc, 0x21, 0xd4, 0x37, 0xc8, 0x0e, 0xaf, 0xa6, 0x45, 0xf4, 0x5e, 0x03, 0xa5, 0xc7,
        0x0a, 0x42, 0xb3, 0x92, 0x8d, 0x7a, 0xf4, 0x29,
    ];

    /// 确定性样本邀请：固定私钥 + 固定 capability（不走 `generate`，那里 capability 是随机的）。
    fn fixed_invite() -> String {
        let sk = swarmdrop_net_base::SecretKey::from_protobuf(FIXED_KEY_PROTOBUF)
            .expect("固定私钥应当可解析");
        crate::PairInvite {
            capability: [
                0x3f, 0x1c, 0xa7, 0x02, 0x9b, 0x64, 0xd8, 0x51, 0x0e, 0xf3, 0x27, 0xbc, 0x45, 0x8a,
                0x16, 0xd0,
            ],
            inviter: swarmdrop_net_base::NodeAddr::with_addrs(
                sk.node_id(),
                vec!["/ip4/192.168.1.10/tcp/4001".parse().unwrap()],
            ),
            expires_at: 1_700_086_400,
            transport_policy: crate::TransportPolicy::Auto,
            display_name: "书房 Mac".into(),
            display_platform: "macos".into(),
        }
        .encode(&sk)
    }

    #[test]
    fn qr_size_baseline() {
        let invite = fixed_invite();
        assert_eq!(
            invite.len(),
            294,
            "样本邀请串长度变了，码面基线需要重新校准"
        );
        assert_eq!(
            build_qr(&invite).unwrap().width(),
            61,
            "码面尺寸偏离基线（61 = QR version 11）"
        );
    }
}
