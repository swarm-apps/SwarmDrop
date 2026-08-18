//! 配对相关渲染。

/// 输出邀请：二维码（可关）+ 链接。
///
/// 链接**总是**输出：手机扫码是主路径，但把码复制到另一台电脑同样常见，而 base32 的
/// 邀请串手输不现实。
pub fn render_invite(invite: &str, json: bool, no_qr: bool) {
    if json {
        let payload = serde_json::json!({ "invite": invite });
        println!("{payload}");
        return;
    }

    if !no_qr {
        match swarmdrop_invite::invite_qr_matrix(invite, crate::render::qr::FACE_PX) {
            Ok(matrix) => println!("{}\n", crate::render::qr::render(matrix.as_slice())),
            // 码画不出来不该让整条命令失败——链接仍然可用。
            Err(err) => eprintln!("二维码生成失败（链接仍可用）: {err}"),
        }
    }

    println!("{invite}");
}

/// 配对完成。
pub fn render_paired(response: &impl std::fmt::Debug, json: bool) {
    if json {
        println!(r#"{{"event":"paired"}}"#);
    } else {
        println!("配对成功");
        tracing::debug!(?response, "配对响应");
    }
}
