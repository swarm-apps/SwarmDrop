//! 网络模块
//!
//! 管理 P2P 节点的启动/关闭、事件循环和运行时网络状态。
//! [`NetManager`] 整合 [`NetClient`](swarm_p2p_core::NetClient)、
//! [`DeviceManager`](crate::device::DeviceManager) 和
//! [`PairingManager`](crate::pairing::manager::PairingManager)，
//! 对外提供统一的网络管理接口。

#![allow(unused_imports)]

pub mod config;
mod event_loop;
mod manager;

pub use event_loop::spawn_event_loop;
pub use manager::{NetManager, NetManagerState, SharedNetRefs};
pub use swarmdrop_core::network::{NatStatus, NetworkStatus, NodeStatus};
