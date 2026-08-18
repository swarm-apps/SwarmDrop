//! 输出渲染：人类可读与机器可读两套。
//!
//! **本层不含业务判断**。它只把已经算好的结果变成字节。任何「要不要显示」「显示哪个」
//! 的决定都属于 [`crate::cmd`]。
//!
//! ## 流向是硬约束
//!
//! | 内容 | 去向 |
//! |---|---|
//! | 结构化结果（`--json`） | **stdout** |
//! | 人类可读结果 | stdout |
//! | 进度、状态行、诊断、日志 | **stderr** |
//!
//! 结构化模式下 stdout 只能有最终结果：混入任何一行进度都会破坏调用方的解析
//! （spec: cli-host「结构化输出模式」）。

pub mod devices;
pub mod inbox;
pub mod pair;
pub mod qr;
pub mod send;
pub mod status;
