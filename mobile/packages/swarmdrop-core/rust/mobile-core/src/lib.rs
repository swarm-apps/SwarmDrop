//! `mobile-core` —— uniffi FFI wrap around `swarmdrop-core` for React Native.
//!
//! ```text
//!   TypeScript (RN)
//!        │  uniffi-bindgen-react-native 生成的 bindings
//!        ▼
//!   mobile-core  (this crate, 按业务领域拆模块)
//!        │  Arc<dyn ForeignTrait> + adapter
//!        ▼
//!   swarmdrop-core  (平台无关业务核心,Tauri 桌面也用同一份)
//!        │
//!        ▼
//!   swarm-p2p-core / sea-orm / ...
//! ```
//!
//! 拆分原则:
//! - 数据 Record/Enum 跟使用它的业务方法放同一文件(event 类型在 events.rs,
//!   pairing 类型在 pairing.rs)
//! - `MobileCore` 主对象的方法分散在各业务模块的 `#[uniffi::export] impl MobileCore { ... }`
//!   块里;Rust 允许多个 impl 块,uniffi proc-macro 都能扫到
//! - 私有 fields 通过 `pub(crate)` 访问器暴露,绝不直接 `pub`
//!
//! 不要做:
//! - 不要给共享 crate `swarmdrop-core` 的 `AppError`/`CoreEvent` 加 uniffi derive
//! - 不要在 RN 调 callback 前持有 Mutex,会死锁
//! - 模块级 async fn 也需要 `#[uniffi::export(async_runtime = "tokio")]`,否则 tokio
//!   reactor 跑不起来

uniffi::setup_scaffolding!();

mod app;
mod device;
mod error;
mod events;
mod file_access;
mod identity;
mod keychain;
mod network;
mod pairing;
mod panic_hook;
mod transfer;
mod utils;
