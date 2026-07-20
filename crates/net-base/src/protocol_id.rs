//! 流协议标识。

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 协议名不合法（必须以 `/` 开头且非空）。
#[derive(Debug, thiserror::Error)]
#[error("invalid protocol id (must start with '/'): {0}")]
pub struct InvalidProtocolId(pub String);

/// 流协议标识，`/swarmdrop/pairing/2` 形态。
///
/// 约定（对齐 libp2p multistream-select 与 iroh ALPN 的共同实践）：
/// - 必须以 `/` 开头（libp2p `StreamProtocol` 的硬要求）；
/// - 必须带版本号结尾——协议匹配是整串精确相等，没有版本协商回退，
///   没版本号就没法演进；
/// - 项目独有前缀（`/swarmdrop/...`）避免与第三方协议撞名。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtocolId(Cow<'static, str>);

impl ProtocolId {
    /// 编译期常量构造。协议名不以 `/` 开头会在编译期 panic。
    pub const fn from_static(s: &'static str) -> Self {
        assert!(
            !s.is_empty() && s.as_bytes()[0] == b'/',
            "protocol id must start with '/'"
        );
        Self(Cow::Borrowed(s))
    }

    /// 运行时构造（动态协议名）。
    pub fn new(s: impl Into<String>) -> Result<Self, InvalidProtocolId> {
        let s = s.into();
        if !s.starts_with('/') {
            return Err(InvalidProtocolId(s));
        }
        Ok(Self(Cow::Owned(s)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 若协议名是 `'static`（`from_static` 构造）则返回它。
    ///
    /// 供内核开流走零分配路径（`libp2p_stream::StreamProtocol::new` 只吃 `&'static str`）；
    /// 本项目全部协议都是编译期常量，故热路径总命中。
    pub fn as_static(&self) -> Option<&'static str> {
        match &self.0 {
            Cow::Borrowed(s) => Some(s),
            Cow::Owned(_) => None,
        }
    }
}

impl fmt::Display for ProtocolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ProtocolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProtocolId({})", self.0)
    }
}

impl Serialize for ProtocolId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProtocolId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 编译期校验：不以 '/' 开头的 from_static 会 const panic（无法写成运行时测试）
    const _PAIRING: ProtocolId = ProtocolId::from_static("/swarmdrop/pairing/2");

    #[test]
    fn runtime_construction_validates_prefix() {
        assert!(ProtocolId::new("/ok/1").is_ok());
        assert!(ProtocolId::new("missing-slash").is_err());
    }
}
