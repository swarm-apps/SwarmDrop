//! 跨模块共享的解析辅助。这里只放纯函数;有状态的 helper 放对应业务模块。

use swarmdrop_net::{Addr, NodeId};

use crate::error::{FfiError, FfiResult};

pub(crate) fn parse_peer_id(value: &str) -> FfiResult<NodeId> {
    value
        .parse()
        .map_err(|error| FfiError::Identity(format!("invalid peer id: {error}")))
}

pub(crate) fn parse_multiaddrs(values: Vec<String>) -> FfiResult<Vec<Addr>> {
    values
        .into_iter()
        .map(|value| {
            value
                .parse()
                .map_err(|error| FfiError::Network(format!("invalid multiaddr: {error}")))
        })
        .collect()
}
