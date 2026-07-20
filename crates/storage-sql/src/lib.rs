//! SwarmDrop 的 SeaORM/SQLite 存储实现——`SessionStore` / `InboxStore` 端口
//! （`swarmdrop-transfer`）的桌面/移动后端。
//!
//! 本 crate 是**实现层**：core 只依赖端口 trait，宿主（src-tauri / mobile-core）在组装点
//! 注入 [`SqlSessionStore`]。Web 端不依赖本 crate（sea-orm 的 sqlx/tokio 链撞 wasm 硬墙，
//! 见 dev-notes/knowledge/storage-abstraction.md）——这正是它从 core 摘出的原因。

pub mod inbox;
pub mod ops;
pub mod store;

pub use store::SqlSessionStore;
