//! 节点装配、单实例仲裁与本地通道。
//!
//! **本层不含任何面向用户的文案**。错误以类型表达，措辞由 [`crate::render`] 决定——
//! 否则同一个失败会在人类可读与结构化两种输出里长出不同的说法。
//!
//! ⚠️ **不得假设调用方是一次性命令**。同一套装配要同时服务常驻的 `start`、一次性的
//! `send`，以及将来可能出现的长期存活的交互式前端（design.md 的 D11）。
//! 具体表现：不要把「干完就退出」的假设写进生命周期，也不要让装配路径依赖进程即将结束。

pub mod boot;
pub mod bootstrap_nodes;
pub mod ipc;
pub mod pairing;
pub mod receive;
pub mod session;
pub mod single;
pub mod transfer;
