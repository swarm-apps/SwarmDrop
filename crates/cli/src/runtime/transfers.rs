//! 传输记录的查询。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）。
//!
//! 记录全在本机的库里，读它不需要网络——所以两条路径都不起节点，有常驻节点时走通道
//! 只是为了避开 SQLite 的写锁（判据见 [`super::access`]）。
//!
//! 函数收 **`&dyn TransferStore` 而不是 `Records`**：常驻节点那侧的 store 已经握在
//! `TransferManager` 手里，收 `Records` 会逼它另开一个数据库连接读同一份数据——于是
//! 通道服务端只能把这几行逻辑再抄一遍（连错误措辞一起）。收端口就两条路径共用一份。

use swarmdrop_core::transfer::store::{TransferProjection, TransferStore};
use uuid::Uuid;

use crate::exit::{CliError, CliResult};

/// 全部传输记录。
///
/// **不在本层重排序**：端口契约已保证按 `started_at` 倒序，而那条契约的存在理由正是
/// 「同一份数据两次调用必须给出同一序」。再排一次只会掩盖端口实现违约的情形。
pub async fn list(store: &dyn TransferStore) -> CliResult<Vec<TransferProjection>> {
    store
        .list_transfer_projections()
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取传输记录失败: {err}")))
}

/// 一条传输记录。
pub async fn show(store: &dyn TransferStore, id: &str) -> CliResult<TransferProjection> {
    let uuid = parse_id(id)?;
    store
        .get_transfer_projection(uuid)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取传输记录失败: {err}")))?
        .ok_or_else(|| CliError::Usage(format!("没有这条传输记录: {id}")))
}

/// 会话标识解析。
///
/// **格式错误是用法错误**，不是「找不到」：前者要用户改参数，后者要用户换一条记录，
/// 而退出码要能区分它们。
pub fn parse_id(id: &str) -> CliResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| CliError::Usage(format!("不是合法的会话标识: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_id_is_a_usage_error() {
        let err = parse_id("not-a-uuid").expect_err("应当拒绝");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    #[test]
    fn well_formed_id_parses() {
        let id = Uuid::new_v4();
        assert_eq!(parse_id(&id.to_string()).expect("解析"), id);
    }
}
