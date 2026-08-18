//! 引导 / 中继节点清单。
//!
//! **这是部署配置，不属于 P2P 内核**——核心不持有任何公共基础设施地址，各宿主按自身
//! transport 能力提供各自的清单。三端各有一份：桌面 `src/lib/bootstrap-nodes.ts`、
//! 移动 `mobile/src/core/bootstrap-nodes.ts`、浏览器 `docs/app/app/_lib/relay-helpers.ts`。
//!
//! 命令行宿主与桌面、移动同属原生端，可用 transport 相同（TCP + QUIC），故清单与它们一致。
//! **浏览器那份不能照抄**：它列的是 webrtc-direct / WebTransport 地址，原生端用不上，
//! 而原生端的裸 TCP 地址浏览器也拨不通。

use swarmdrop_core::network::NetworkRuntimeConfig;

/// 自建引导节点（同时是中继）。
const BOOTSTRAP_NODES: &[&str] = &[
    "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
    "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/// 命令行宿主的默认网络配置。
///
/// `provide_lan_helper` 保持关闭：局域网协助节点会为同网设备转发流量，那是用户该显式
/// 选择的事，不该因为「跑了个命令行」就默默承担。
pub fn default_network_config() -> NetworkRuntimeConfig {
    NetworkRuntimeConfig {
        bootstrap_nodes: BOOTSTRAP_NODES.iter().map(|s| (*s).to_string()).collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 清单里每条都必须能解析出节点标识——地址写错时，症状是「连不上引导节点」，
    /// 而那与网络故障无法区分，只有这条测试能在提交前拦住它。
    #[test]
    fn every_bootstrap_address_parses() {
        let config = default_network_config();
        assert!(!config.bootstrap_nodes.is_empty(), "清单不得为空");
        for raw in &config.bootstrap_nodes {
            let addr: swarmdrop_net::Addr = raw.parse().expect("引导地址无法解析为 multiaddr");
            assert!(
                addr.p2p_node_id().is_some(),
                "引导地址缺少 /p2p 段，拨过去无法校验对端身份: {raw}"
            );
        }
    }
}
