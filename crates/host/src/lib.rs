//! swarmdrop-host：宿主端口层（platform-neutral ports + DTO + error + device 数据类型）。
//!
//! 从 swarmdrop-core 下沉，供 `swarmdrop-core` 与 `swarmdrop-transfer` 共同依赖。刻意
//! 保持轻依赖（net-base + entity + sea-orm 仅类型宏），wasm 双 target 可编（进
//! `scripts/check-wasm.sh`）。
//!
//! 端口共六个（[`ports`]）：[`KeychainProvider`]（密钥材料）、[`PairedDeviceStore`]
//! （已配对设备列表 —— 与前者拆开的理由写在 trait 文档里）、[`DeviceConfig`]（用户设备名）、
//! [`FileAccess`]、[`Notifier`]、[`UpdateInstaller`]。
//!
//! 事件聚合（`CoreEvent` / `EventBus`）与测试用 `MemoryHost` **不在本 crate**——它们
//! 引用 network / transfer 域的 DTO（含 transfer wire 类型），下沉到端口层会成环，
//! 故留在 `swarmdrop-core`。
//!
//! 端口的**平台中立默认实现**也放这里，目前只有一个：[`JsonFileDeviceConfig`]（桌面与
//! 移动共用的 `device_config.json` 读写）。判据是「多个宿主会写出逐行同构的同一份实现，
//! 且它们的真差异只在构造参数上」——理由与门控见
//! [`device_config_file`] 的模块文档。

pub mod device;
#[cfg(not(target_family = "wasm"))]
pub mod device_config_file;
pub mod error;
pub mod ports;

#[cfg(not(target_family = "wasm"))]
pub use device_config_file::JsonFileDeviceConfig;
pub use error::{AppError, AppResult};
pub use ports::*;
