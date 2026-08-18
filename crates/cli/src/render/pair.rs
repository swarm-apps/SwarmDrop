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

/// 等待对方扫码。**写 stderr**：它是过程信息，不是命令结果。
pub fn render_waiting(json: bool) {
    if !json {
        eprintln!("等待对方扫码配对…（Ctrl-C 取消）");
        eprintln!("注意：这张码在本命令退出后即失效——邀请的可拨地址就是当前这个进程的节点。");
    }
}

/// 配对完成，且知道对方是谁。
pub fn render_paired_with(os_info: &swarmdrop_core::device::OsInfo, json: bool) {
    let name = os_info
        .name
        .clone()
        .unwrap_or_else(|| os_info.hostname.clone());
    if json {
        let payload = serde_json::json!({ "event": "paired", "device": name });
        println!("{payload}");
    } else {
        println!("配对成功：{name}");
    }
}

/// 配对完成。
pub fn render_paired(json: bool) {
    if json {
        println!(r#"{{"event":"paired"}}"#);
    } else {
        println!("配对成功");
    }
}
