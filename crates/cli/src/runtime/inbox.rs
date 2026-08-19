//! 收件箱的查询。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）——除了错误消息，那是 [`CliError`]
//! 的一部分，两条路径必须给出同一句话与同一个分类。
//!
//! 函数收 **`&dyn TransferStore` 而不是 `Records`**：常驻节点那侧的 store 已经握在
//! `TransferManager` 手里，收 `Records` 会逼它另开一个数据库连接读同一份数据——于是
//! 通道服务端只能把这几行再抄一遍。收端口就两条路径共用一份。
//!
//! ⚠️ **标识解析不要借用 [`super::transfers::parse_id`]**：那条的措辞是「不是合法的
//! **会话**标识」，而这里的对象是收件箱**条目**。借用过一次，`inbox show 乱码` 就报出了
//! 会话的名词，而常驻路径仍说条目——同一条命令、同一个输入、两个名词。

use swarmdrop_core::transfer::inbox::{InboxItemDetail, InboxItemSummary};
use swarmdrop_core::transfer::store::TransferStore;
use uuid::Uuid;

use crate::exit::{CliError, CliResult};

/// 收件箱条目清单。
pub async fn list(store: &dyn TransferStore) -> CliResult<Vec<InboxItemSummary>> {
    store
        .list_inbox_items(false)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取收件箱失败: {err}")))
}

/// 一个条目的详情。
///
/// 「没有这个条目」是**用法错误**（换一个标识重来），不是「节点不可用」——
/// 两者的退出码不同，而脚本靠退出码判断。
pub async fn detail(store: &dyn TransferStore, id: &str) -> CliResult<InboxItemDetail> {
    let uuid = parse_id(id)?;
    store
        .get_inbox_item_detail(uuid)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取条目失败: {err}")))?
        .ok_or_else(|| CliError::Usage(format!("收件箱里没有条目 {id}")))
}

/// 条目标识解析。
///
/// **格式错误是用法错误**，不是「找不到」：前者要用户改参数，后者要用户换一个条目。
pub fn parse_id(id: &str) -> CliResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| CliError::Usage(format!("不是合法的条目标识: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 措辞说的必须是**条目**，不是会话。
    ///
    /// 这条看守的是一次真实的回归：`cmd/inbox.rs` 曾经复用
    /// [`super::transfers::parse_id`]，于是 `inbox show 乱码` 报出了「不是合法的会话
    /// 标识」，而同一条命令走常驻节点时说的是「条目标识」。复用要看语义，不只是看形状。
    #[test]
    fn malformed_id_names_the_right_noun() {
        let err = parse_id("not-a-uuid").expect_err("应当拒绝");
        assert_eq!(err.code(), crate::exit::Code::Usage);
        assert!(
            err.to_string().contains("条目"),
            "措辞应指向收件箱条目，实际: {err}"
        );
    }

    #[test]
    fn well_formed_id_parses() {
        let id = Uuid::new_v4();
        assert_eq!(parse_id(&id.to_string()).expect("解析"), id);
    }
}
