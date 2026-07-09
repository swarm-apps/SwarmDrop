//! 基础设施链路收敛（声明式收敛家族第二员，与 [`crate::presence`] 同模式）。
//!
//! 对候选表（[`BootstrapCandidateManager`]）中每个具备 relay 角色的候选，
//! 持续维持「relay reservation 存活」这一期望状态：
//!
//! - reservation 丢失（[`NodeEvent::RelayReservationLost`]）→ 退避重建
//!   （[`ensure_relay_reservation`](swarm_p2p_core::NetClient::ensure_relay_reservation)
//!   幂等原语，连带重连与 identify 触发的自动重建）；
//! - 运行时经 identify 学到的基础设施节点（`is_bootstrap_agent`）自动纳管
//!   （来源 [`BootstrapCandidateSource::Learned`]）——LanOnly 设备经
//!   LAN Helper 认识公网中继的那一刻即进入收敛清单；
//! - 公网范围候选的 reservation 受 `public_reachability` 设置约束，
//!   LAN 范围（LAN Helper）不受限。
//!
//! Kad 接线与首次注册仍由既有的事件驱动路径（`maybe_register_lan_helper` /
//! `connect_bootstrap_peers`）即时完成；本模块是它们的收敛兜底——
//! 一次性接线断了以后，这里负责把世界拉回期望状态。

mod supervisor;

pub use supervisor::InfraSupervisor;
