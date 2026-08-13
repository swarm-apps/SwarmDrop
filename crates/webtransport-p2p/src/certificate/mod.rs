//! 证书生命周期。
//!
//! 两层，职责不重叠：
//!
//! | 模块 | 管什么 | 知道时间吗 |
//! |---|---|---|
//! | [`cert`] | 单张证书：生成、PEM 往返、certhash、spec 合规性 | 只知道自己的有效期窗口 |
//! | [`rotation`] | 两张证书的角色与更替、通告集合、Noise 上报集合 | 时间从 `advance(now)` 的参数进来 |
//!
//! 两者都**不做 IO、不认识 libp2p 的 `Transport`**。持久化由宿主经
//! [`crate::CertificateStore`] 端口完成，本模块只产出/消费字符串。

mod cert;
mod rotation;

pub use cert::{Certificate, Error, MAX_VALIDITY};
pub use rotation::{Advance, Rotation};
