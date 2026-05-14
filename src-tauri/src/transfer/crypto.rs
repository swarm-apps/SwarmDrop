//! 传输加密模块
//!
//! 使用 XChaCha20-Poly1305 对文件分块进行端到端加密。
//! 每次传输生成独立的 256-bit 对称密钥，传输结束后销毁。
//!
//! ## Nonce 派生
//!
//! 使用 BLAKE3 `derive_key` 模式从 `(session_id, file_id, chunk_index)` 确定性派生
//! 24 字节 nonce，支持乱序、并发和重试场景，无需同步计数器。

use chacha20poly1305::aead::{self, Aead};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use uuid::Uuid;

/// 传输加密器
///
/// 封装 XChaCha20-Poly1305 AEAD，提供基于 `(session_id, file_id, chunk_index)`
/// 的确定性 nonce 派生加密/解密接口。
///
/// 密钥仅存于内存中，传输结束后随结构体一起销毁。
pub struct TransferCrypto {
    cipher: XChaCha20Poly1305,
}

impl TransferCrypto {
    /// 从 256-bit 密钥创建加密器
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// 加密分块（发送方调用）
    ///
    /// 输出 = 密文 + 16 字节 Poly1305 认证标签
    pub fn encrypt_chunk(
        &self,
        session_id: &Uuid,
        file_id: u32,
        chunk_index: u32,
        plaintext: &[u8],
    ) -> aead::Result<Vec<u8>> {
        let nonce = derive_nonce(session_id, file_id, chunk_index);
        self.cipher.encrypt(XNonce::from_slice(&nonce), plaintext)
    }

    /// 解密分块（接收方调用）
    ///
    /// 先验证 Poly1305 认证标签，通过后再解密。
    /// 如果数据被篡改，返回 `DecryptionFailed`。
    pub fn decrypt_chunk(
        &self,
        session_id: &Uuid,
        file_id: u32,
        chunk_index: u32,
        ciphertext: &[u8],
    ) -> aead::Result<Vec<u8>> {
        let nonce = derive_nonce(session_id, file_id, chunk_index);
        self.cipher.decrypt(XNonce::from_slice(&nonce), ciphertext)
    }
}

/// 从 `(session_id, file_id, chunk_index)` 派生 24 字节 nonce
///
/// 使用 BLAKE3 `derive_key` 模式：
/// - context 字符串做域分离，防止与其他用途碰撞
/// - 输入为 `session_id (16 bytes) || file_id (4 bytes BE) || chunk_index (4 bytes BE)`
/// - 输出 32 字节，截取前 24 字节作为 XChaCha20 的 nonce
///
/// 确定性派生保证：相同输入 → 相同 nonce（幂等），不同输入 → 不同 nonce（安全）。
fn derive_nonce(session_id: &Uuid, file_id: u32, chunk_index: u32) -> [u8; 24] {
    let mut input = [0u8; 24];
    input[..16].copy_from_slice(session_id.as_bytes());
    input[16..20].copy_from_slice(&file_id.to_be_bytes());
    input[20..24].copy_from_slice(&chunk_index.to_be_bytes());

    let hash = blake3::derive_key("swarmdrop-transfer-nonce-v1", &input);

    hash[..24].try_into().expect("blake3 output >= 24 bytes")
}

/// 生成随机 256-bit 加密密钥
pub fn generate_key() -> [u8; 32] {
    use chacha20poly1305::aead::OsRng;
    XChaCha20Poly1305::generate_key(&mut OsRng).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uuid() -> Uuid {
        Uuid::parse_str("a1b2c3d4-e5f6-4789-abcd-ef0123456789").unwrap()
    }

    fn test_uuid2() -> Uuid {
        Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap()
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();
        let plaintext = b"hello, swarmdrop!";

        let ciphertext = crypto.encrypt_chunk(&sid, 0, 0, plaintext).unwrap();

        // 密文应比明文长（+16 字节认证标签）
        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        let decrypted = crypto.decrypt_chunk(&sid, 0, 0, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1 = generate_key();
        let key2 = generate_key();
        let crypto1 = TransferCrypto::new(&key1);
        let crypto2 = TransferCrypto::new(&key2);
        let sid = test_uuid();

        let ciphertext = crypto1.encrypt_chunk(&sid, 0, 0, b"secret data").unwrap();

        let result = crypto2.decrypt_chunk(&sid, 0, 0, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_with_wrong_nonce_params_fails() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();
        let sid2 = test_uuid2();

        let ciphertext = crypto.encrypt_chunk(&sid, 0, 0, b"data").unwrap();

        // session_id 不同
        assert!(crypto.decrypt_chunk(&sid2, 0, 0, &ciphertext).is_err());
        // file_id 不同
        assert!(crypto.decrypt_chunk(&sid, 1, 0, &ciphertext).is_err());
        // chunk_index 不同
        assert!(crypto.decrypt_chunk(&sid, 0, 1, &ciphertext).is_err());
    }

    #[test]
    fn same_input_produces_same_ciphertext() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();
        let plaintext = b"idempotent data";

        let ct1 = crypto.encrypt_chunk(&sid, 0, 5, plaintext).unwrap();
        let ct2 = crypto.encrypt_chunk(&sid, 0, 5, plaintext).unwrap();

        // 确定性 nonce → 同一明文加密结果一致（幂等安全）
        assert_eq!(ct1, ct2);
    }

    #[test]
    fn different_chunks_produce_different_ciphertext() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();
        let plaintext = b"same data";

        let ct0 = crypto.encrypt_chunk(&sid, 0, 0, plaintext).unwrap();
        let ct1 = crypto.encrypt_chunk(&sid, 0, 1, plaintext).unwrap();

        // 不同 chunk_index → 不同 nonce → 不同密文
        assert_ne!(ct0, ct1);
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();

        let mut ciphertext = crypto.encrypt_chunk(&sid, 0, 0, b"important data").unwrap();

        // 篡改密文的第一个字节
        ciphertext[0] ^= 0xff;

        let result = crypto.decrypt_chunk(&sid, 0, 0, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn large_chunk_roundtrip() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();

        // 模拟 256KB 分块
        let plaintext: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();

        let ciphertext = crypto.encrypt_chunk(&sid, 3, 42, &plaintext).unwrap();

        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        let decrypted = crypto.decrypt_chunk(&sid, 3, 42, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let key = generate_key();
        let crypto = TransferCrypto::new(&key);
        let sid = test_uuid();

        let ciphertext = crypto.encrypt_chunk(&sid, 0, 0, b"").unwrap();

        // 空明文仍有 16 字节认证标签
        assert_eq!(ciphertext.len(), 16);

        let decrypted = crypto.decrypt_chunk(&sid, 0, 0, &ciphertext).unwrap();

        assert!(decrypted.is_empty());
    }

    #[test]
    fn nonce_deterministic() {
        let sid = test_uuid();
        let n1 = derive_nonce(&sid, 1, 2);
        let n2 = derive_nonce(&sid, 1, 2);
        assert_eq!(n1, n2);
    }

    #[test]
    fn nonce_differs_on_any_input_change() {
        let sid = test_uuid();
        let sid2 = test_uuid2();
        let base = derive_nonce(&sid, 0, 0);

        assert_ne!(base, derive_nonce(&sid2, 0, 0));
        assert_ne!(base, derive_nonce(&sid, 1, 0));
        assert_ne!(base, derive_nonce(&sid, 0, 1));
    }
}
