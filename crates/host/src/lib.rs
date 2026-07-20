//! swarmdrop-host：宿主端口层（platform-neutral ports + DTO + error + device 数据类型）。
//!
//! 从 swarmdrop-core 下沉，供 `swarmdrop-core` 与 `swarmdrop-transfer` 共同依赖。刻意
//! 保持轻依赖（net-base + entity + sea-orm 仅类型宏），wasm 双 target 可编（进
//! `scripts/check-wasm.sh`）。
//!
//! 事件聚合（`CoreEvent` / `EventBus`）与测试用 `MemoryHost` **不在本 crate**——它们
//! 引用 network / transfer 域的 DTO（含 transfer wire 类型），下沉到端口层会成环，
//! 故留在 `swarmdrop-core`。

pub mod device;
pub mod error;
pub mod ports;

pub use error::{AppError, AppResult};
pub use ports::*;
