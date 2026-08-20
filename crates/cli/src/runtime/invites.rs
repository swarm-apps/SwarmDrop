//! 已发出邀请的清点与撤销。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）：措辞由 [`crate::render`] 决定。
//!
//! ## 为什么这条路径不需要节点
//!
//! 邀请注册表的内存态是一次性消费的权威判定点，但它的内容全部写穿到了本机的库里。
//! 无节点时建一个孤立的注册表 + `load` 就能得到同一份记录——这正是节点启动时做的事
//! （core 组合根的 `load_invites`）。
//!
//! 这不是便利性考虑：邀请 TTL 24 小时且跨重启存活，发现泄露时撤销是唯一有效的处置，
//! 而那一刻用户很可能并未启动节点。把清点绑在运行中的节点上，等于要求用户在止损前先
//! 完成一次可能失败的启动。

use serde::{Deserialize, Serialize};
use swarmdrop_invite::{InviteRegistry, InviteSummary, capability_hash_from_hex};

/// 邀请清单的一条。
///
/// 两条取数路径（常驻节点经通道 / 无节点直连记录）**共用这一个形状**——否则同一件事
/// 会在两条路径上长出两种表达，而只有其中一条会被测到。
///
/// **不含邀请串本身**：capability 明文不落盘也不出注册表（`invite-persistence` design D4），
/// 重启后连原始链接都拼不回来。想再分享只能重新生成一张。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRow {
    /// `sha256(capability)` 的小写 hex。撤销时按它定位，其余场合只当不透明标识。
    pub id: String,
    pub created_at: u64,
    pub expires_at: u64,
    /// 已被对方使用（仍列到过期为止，好让用户看到这张的去向）。
    pub consumed: bool,
}

impl From<InviteSummary> for InviteRow {
    fn from(summary: InviteSummary) -> Self {
        Self {
            id: swarmdrop_invite::capability_hash_to_hex(&summary.capability_hash),
            created_at: summary.created_at,
            expires_at: summary.expires_at,
            consumed: summary.consumed,
        }
    }
}

/// 列出未过期、未撤销的邀请（最近生成的在前）。
///
/// 过滤与排序都由注册表的 `list_active` 承担，本函数只做形状转换——**领域规则不在这里
/// 复述一遍**，那会长出第二份实现，而两份迟早会不一致。
pub fn list(registry: &InviteRegistry, now: u64) -> Vec<InviteRow> {
    registry
        .list_active(now)
        .into_iter()
        .map(InviteRow::from)
        .collect()
}

/// 一次撤销的结局。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeOutcome {
    /// 撤销了几张。
    ///
    /// ⚠️ 严格说是「**送去撤销**的条数」：`revoke_by_hash` 报的是有没有写穿，不是有没有
    /// 命中——一张在「取清单」与「执行撤销」之间刚被用掉或过期的邀请，这里照样计数。
    /// 正常路径上不会偏（客户端撤的就是它刚列出来的那几张），窗口只在那两步之间。
    /// 要它精确得让 `revoke_by_hash` 一并返回「找到了没有」，那是 `crates/invite` 的改动。
    pub revoked: usize,
    /// 是否**全部**已写入持久化存储。
    ///
    /// `false` 意味着撤销在本次运行内生效了，但重启后那些邀请会复活（写穿失败，库里仍是
    /// 生成时写下的 pending）。必须如实告知——否则用户以为已经撤销干净了。
    pub persisted: bool,
}

/// 撤销指定的若干张。
///
/// **不因某一张写穿失败而短路**：那正是用户要求撤掉的其余几张。汇总的 `persisted`
/// 取合取——只要有一张没落盘，就得如实警告「重启后会复活」。
///
/// 任一标识不合法时整条失败（返回 `None` 并带上那个标识）：批量撤销不可逆，
/// 敲错一个的正确处置是停下来，而不是撤掉另外几张之后再说有一个没认出来。
pub async fn revoke_each<'a>(
    registry: &InviteRegistry,
    hexes: impl IntoIterator<Item = &'a str>,
) -> Result<RevokeOutcome, &'a str> {
    let mut outcome = RevokeOutcome {
        revoked: 0,
        persisted: true,
    };
    for hex in hexes {
        let hash = capability_hash_from_hex(hex).ok_or(hex)?;
        outcome.persisted &= registry.revoke_by_hash(hash).await;
        outcome.revoked += 1;
    }
    Ok(outcome)
}

/// 撤销全部未过期的邀请。
///
/// 逐条撤而非「清空表」：撤销要落成 `Revoked` 状态而非删行（已撤销与已被使用在列表里要能
/// 区分），而那是 `revoke_by_hash` 的职责。
pub async fn revoke_all(registry: &InviteRegistry, now: u64) -> RevokeOutcome {
    let mut revoked = 0usize;
    let mut persisted = true;
    // **直接取 summary 的哈希，不经 hex 往返**：往返会多出一条「解析失败就跳过、且不计数」
    // 的分支，而通道那侧（`cmd/start.rs` 的 `InviteRevokeAll`）没有它——同一条 `--all`
    // 于是可能在两条路径上报出不同的撤销条数。那条分支今天永不触发（hex 编解码无损），
    // 但没有任何东西保证它明天还不触发。
    for summary in registry.list_active(now) {
        // **不能短路**：某一张写穿失败不该让后面的都不撤——那正是「全撤」要防的情形。
        persisted &= registry.revoke_by_hash(summary.capability_hash).await;
        revoked += 1;
    }
    RevokeOutcome { revoked, persisted }
}

/// 用户敲的标识前缀短于此即拒绝。
///
/// 防的是「手滑敲了一个字符就撤掉了什么」。4 位在 16 进制下是 65536 种，
/// 而个人用途的未过期邀请通常是个位数条。
pub const MIN_PREFIX: usize = 4;

/// 前缀解析失败的原因。
///
/// 分三种而不是一个笼统的「找不到」：三者的用户动作完全不同——补齐位数、换一个标识、
/// 从候选里挑一个。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixError {
    /// 短于 [`MIN_PREFIX`]。
    TooShort,
    /// 一张都没匹配上。
    NotFound,
    /// 匹配到多张。附全部候选，让用户能直接从中选一个补齐。
    Ambiguous(Vec<String>),
}

/// 按标识前缀在当前邀请集合里定位唯一一张。
///
/// **歧义绝不代为消解**：撤销不可逆，猜错没有补救。匹配到多张时返回全部候选，
/// 由调用方原样呈现。
///
/// 大小写不敏感：标识是 hex，用户从终端复制时大小写可能被变换（某些终端与
/// 剪贴板管理器会这么干）。
pub fn resolve_prefix<'a>(
    rows: &'a [InviteRow],
    prefix: &str,
) -> Result<&'a InviteRow, PrefixError> {
    if prefix.len() < MIN_PREFIX {
        return Err(PrefixError::TooShort);
    }
    let needle = prefix.to_ascii_lowercase();

    let matched: Vec<&InviteRow> = rows
        .iter()
        .filter(|row| row.id.starts_with(&needle))
        .collect();

    match matched.as_slice() {
        [] => Err(PrefixError::NotFound),
        [only] => Ok(only),
        many => Err(PrefixError::Ambiguous(
            many.iter().map(|row| row.id.clone()).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> InviteRow {
        InviteRow {
            id: id.into(),
            created_at: 1_800_000_000,
            expires_at: 1_800_086_400,
            consumed: false,
        }
    }

    #[test]
    fn unique_prefix_resolves() {
        let rows = [row("abcd1234"), row("ef567890")];
        assert_eq!(resolve_prefix(&rows, "abcd").unwrap().id, "abcd1234");
    }

    /// 大小写不敏感——终端与剪贴板管理器会变换 hex 的大小写。
    #[test]
    fn prefix_is_case_insensitive() {
        let rows = [row("abcd1234")];
        assert_eq!(resolve_prefix(&rows, "ABCD").unwrap().id, "abcd1234");
    }

    /// **撞车时必须拒绝并给出全部候选**，不得任选其一。
    ///
    /// 这条看守的是一个不可逆操作：猜错了撤掉的是另一张邀请，而它没有 undo。
    #[test]
    fn ambiguous_prefix_is_refused_with_candidates() {
        let rows = [row("abcd1111"), row("abcd2222"), row("ef000000")];
        match resolve_prefix(&rows, "abcd") {
            Err(PrefixError::Ambiguous(candidates)) => {
                assert_eq!(candidates.len(), 2);
                assert!(candidates.contains(&"abcd1111".to_string()));
                assert!(candidates.contains(&"abcd2222".to_string()));
            }
            other => panic!("撞车必须拒绝，实际: {other:?}"),
        }
    }

    /// 太短一律拒绝——防手滑撤掉一张没打算撤的。
    #[test]
    fn too_short_prefix_is_refused() {
        let rows = [row("abcd1234")];
        assert_eq!(resolve_prefix(&rows, "abc"), Err(PrefixError::TooShort));
        // 边界：正好 MIN_PREFIX 位可用。
        assert!(resolve_prefix(&rows, "abcd").is_ok());
    }

    #[test]
    fn unknown_prefix_is_not_found() {
        let rows = [row("abcd1234")];
        assert_eq!(resolve_prefix(&rows, "9999"), Err(PrefixError::NotFound));
    }

    /// 清单条目要能经通道往返——两端是独立编译的代码路径，形状对不上时表现是
    /// 「命令卡住」而不是编译错误。
    #[test]
    fn row_round_trips() {
        let value = serde_json::to_value(row("abcd1234")).expect("编码");
        let back: InviteRow = serde_json::from_value(value).expect("往返");
        assert_eq!(back.id, "abcd1234");
        assert!(!back.consumed);
    }
}
