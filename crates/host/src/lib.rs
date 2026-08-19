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
//! **本 crate 是纯端口：零文件 IO、零平台实现。** 端口的 native 本地文件系统实现住在
//! `swarmdrop-host-fs`（身份存储 / 设备配置 / 文件读写），由各宿主自行依赖。
//!
//! 那次拆分的判据：同一份实现被三个以上宿主逐行同构地各写一遍时，它属于共享实现而非
//! 端口；而端口层一旦开始承载实现，「本 crate 要过 wasm 双 target 门禁」这条约束就会
//! 反过来污染实现的写法（同步 IO、target-specific 依赖、cfg 门控）。

pub mod device;
pub mod error;
pub mod notification;
pub mod ports;
pub mod time;

pub use error::{AppError, AppResult};
pub use notification::{
    SystemNotification, SystemNotificationPublisher, publish_if_window_unfocused,
};
pub use ports::*;
pub use time::now_secs;
