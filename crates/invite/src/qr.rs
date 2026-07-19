//! 邀请二维码生成（三端统一规范，wasm-clean）。
//!
//! 三端渲染的唯一编码源——把 QR 的编码策略单点固化，避免各端 JS 库各写一遍导致漂移
//! （openspec: pair-invite-ui design D1/D2）。三端拿本模块产出的 SVG 字符串（web 直接
//! `innerHTML`、桌面 `dangerouslySetInnerHTML`）或模块矩阵（RN 用 react-native-svg 自绘）。
//!
//! 关键规范（全部固化在此，调用方传小写规范邀请串即可）：
//! - **payload 大写化** → 走 QR alphanumeric 模式（base32 大写字母表 `A-Z2-7` 全落在
//!   alphanumeric 字符集）：byte 模式 v13-15 降到 v11-12，模块数 -15%，扫码可靠性显著↑。
//!   解码大小写不敏感（见 [`crate::PairInvite::decode`]），零风险。
//! - **ECL::M**（15% 纠错）：屏→摄像头近距干净场景足够；Q/H 只会顶高版本更难扫，且无 logo。
//! - **quiet zone 4 模块**（ISO 硬性要求）。
//! - 配色由渲染端负责：**深模块 + 白底，不随暗色主题反色**（摄像头对反色 QR 识别差）。

use fast_qr::{ECL, QRBuilder, QRCode};

/// 三端统一的 QR 编码：payload 大写化（→ alphanumeric）+ ECL::M。**编码策略单点**——
/// SVG / matrix 两个渲染出口都经此，改 ECL/模式只此一处。
fn build_qr(invite: &str) -> Result<QRCode, QrError> {
    QRBuilder::new(invite.to_ascii_uppercase())
        .ecl(ECL::M)
        .build()
        .map_err(|e| QrError(e.to_string()))
}

/// 生成邀请二维码的 SVG 字符串（深模块 `#0a0a0a`，透明背景——渲染端套白卡）。
///
/// `invite` 传小写规范邀请串（`PairInvite::encode` 的产物）；本函数内部大写化。
pub fn invite_qr_svg(invite: &str) -> Result<String, QrError> {
    use fast_qr::convert::{Builder, Shape, svg::SvgBuilder};

    let qr = build_qr(invite)?;
    Ok(SvgBuilder::default()
        .shape(Shape::Square)
        .margin(4)
        .module_color([10, 10, 10, 255])
        .to_str(&qr))
}

/// 生成邀请二维码的模块矩阵（`true` = 深模块）。RN 端按此自绘 `<Rect>`；
/// 已含 4 模块 quiet zone 边距。
pub fn invite_qr_matrix(invite: &str) -> Result<Vec<Vec<bool>>, QrError> {
    let qr = build_qr(invite)?;
    let size = qr.size;
    const QZ: usize = 4;
    let full = size + QZ * 2;
    let mut matrix = vec![vec![false; full]; full];
    for (r, row) in qr.data.chunks(size).take(size).enumerate() {
        for (c, module) in row.iter().enumerate() {
            matrix[r + QZ][c + QZ] = module.value();
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
        let svg = invite_qr_svg(&sample_invite()).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("path") || svg.contains("rect"));
    }

    #[test]
    fn matrix_is_square_with_quiet_zone() {
        let m = invite_qr_matrix(&sample_invite()).unwrap();
        assert!(!m.is_empty());
        assert!(m.iter().all(|row| row.len() == m.len()), "矩阵必须是正方形");
        // 四角属 quiet zone，必为浅模块
        assert!(!m[0][0] && !m[0][m.len() - 1]);
    }

    /// 大写化 → alphanumeric 模式降版本：同一 payload 大写比小写 QR version 更低（模块更少）。
    #[test]
    fn uppercase_lowers_qr_version() {
        let invite = sample_invite();
        let upper = QRBuilder::new(invite.to_ascii_uppercase())
            .ecl(ECL::M)
            .build()
            .unwrap();
        let lower = QRBuilder::new(invite.clone()).ecl(ECL::M).build().unwrap();
        assert!(
            upper.size < lower.size,
            "大写 alphanumeric 应比小写 byte 模式版本更低: upper={} lower={}",
            upper.size,
            lower.size
        );
    }
}
