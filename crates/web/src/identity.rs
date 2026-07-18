//! 身份持久化：`SecretKey` 的 protobuf 编码经 hex 存 localStorage，启动恢复。
//!
//! 范围内不做配对持久化，但节点身份必须稳定（circuit 地址 / 分享码发布都绑 NodeId），
//! 故最小地存一份密钥。protobuf 编码与桌面/移动 keychain 存量同构。

use swarmdrop_net::SecretKey;

use crate::error::{WebError, WebResult};

const STORAGE_KEY: &str = "swarmdrop.identity.protobuf.hex";

/// 从 localStorage 恢复身份；缺失 / 损坏则生成新身份并写回。
pub fn load_or_create() -> WebResult<SecretKey> {
    let storage = local_storage()?;

    if let Ok(Some(hex)) = storage.get_item(STORAGE_KEY)
        && let Some(bytes) = decode_hex(&hex)
        && let Ok(sk) = SecretKey::from_protobuf(&bytes)
    {
        return Ok(sk);
    }

    let sk = SecretKey::generate();
    let hex = encode_hex(&sk.to_protobuf());
    storage
        .set_item(STORAGE_KEY, &hex)
        .map_err(|_| WebError::storage("写入 localStorage 身份失败"))?;
    Ok(sk)
}

fn local_storage() -> WebResult<web_sys::Storage> {
    web_sys::window()
        .ok_or_else(|| WebError::storage("无 window（非浏览器主线程？）"))?
        .local_storage()
        .map_err(|_| WebError::storage("localStorage 不可用"))?
        .ok_or_else(|| WebError::storage("localStorage 为空"))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    (0..bytes.len() / 2)
        .map(|i| {
            let hi = (bytes[2 * i] as char).to_digit(16)?;
            let lo = (bytes[2 * i + 1] as char).to_digit(16)?;
            Some((hi * 16 + lo) as u8)
        })
        .collect()
}
