//! Epoch 准入判定单点。
//!
//! 传输生命周期用单调递增的 `epoch` 区分「同一会话的不同代」（首传 epoch=0，每次
//! resume `new_epoch = max(local, peer) + 1`）。多处需要按 epoch 比较，但语义各不相同——
//! 集中命名，避免 `<` / `>` / `==` 散落各处、读代码时还要反推方向：
//! - **迟到**（reducer 忽略旧 actor / network 事件）：`incoming < current`
//! - **更新**（registry 替换旧 actor、resume commit 推进 epoch）：`incoming > current`
//! - **精确匹配**（data-channel Hello 帧只接受当前 epoch）：`incoming == current`

/// Epoch 比较的命名判定（纯函数，无状态）。
pub struct EpochGuard;

impl EpochGuard {
    /// 迟到：`incoming` 早于 `current`，应忽略（reducer 防旧消息污染）。
    pub fn is_stale(incoming: i64, current: i64) -> bool {
        incoming < current
    }

    /// 更新：`incoming` 严格晚于 `current`，应推进 / 替换（registry 替换、resume commit）。
    pub fn is_newer(incoming: i64, current: i64) -> bool {
        incoming > current
    }

    /// 精确匹配当前 epoch（data-channel Hello 帧过滤，旧 / 新代一律拒）。
    pub fn matches(incoming: i64, current: i64) -> bool {
        incoming == current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_newer_match_are_consistent() {
        // 迟到与更新互斥，且都不含相等。
        assert!(EpochGuard::is_stale(1, 2));
        assert!(!EpochGuard::is_stale(2, 2));
        assert!(!EpochGuard::is_stale(3, 2));

        assert!(EpochGuard::is_newer(3, 2));
        assert!(!EpochGuard::is_newer(2, 2));
        assert!(!EpochGuard::is_newer(1, 2));

        assert!(EpochGuard::matches(2, 2));
        assert!(!EpochGuard::matches(1, 2));
        assert!(!EpochGuard::matches(3, 2));
    }
}
