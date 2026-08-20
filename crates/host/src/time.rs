//! 平台中立的当前时刻。
//!
//! 收在这里而不是各处自取，理由是**两个 target 的实现不一样**：
//! `std::time::SystemTime::now()` 在 `wasm32-unknown-unknown` 上没有可用的时钟源
//! （返回值不可靠，且部分工具链下直接 panic），而 chrono 在 wasm 下走 JS 的
//! `Date.now()`。散在各处自取的写法在 native 上一切正常，只有编到浏览器 target 时
//! 才暴露——那时故障点是「邀请刚生成就判为过期」这类离时钟很远的症状。
//!
//! 秒而非毫秒：本仓按 Unix 秒计时的只有邀请 TTL 与配对时刻，两者都在
//! [`swarmdrop_invite`] 的 API 边界上以 `u64` 秒出现。

/// 当前 Unix 秒。
///
/// 系统时钟早于 1970 时取 `0`——那只可能是时钟被拨错，而返回一个负数换算的巨大 `u64`
/// 会让「一切都已过期」和「一切都永不过期」随符号翻转随机发生。
pub fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 返回值必须落在一个合理的年代区间内。
    ///
    /// 看守的是「换了实现后单位从秒变成毫秒」这类改动：数值仍是正的、代码仍能编过，
    /// 但所有 TTL 判定会一起失真 1000 倍。
    #[test]
    fn now_is_a_plausible_unix_timestamp() {
        let now = now_secs();
        assert!(now > 1_700_000_000, "早于 2023，疑似单位或纪元错误: {now}");
        assert!(now < 4_000_000_000, "晚于 2096，疑似单位错误: {now}");
    }
}
