//! 宿主端口的 native 本地文件系统实现。
//!
//! [`swarmdrop_host`] 定义端口（trait + DTO），本 crate 提供它们在「有本地文件系统」
//! 这个前提下的实现——**多个原生宿主逐行同构的那一份**。
//!
//! ## 判据：什么该进来
//!
//! 「多个宿主会写出逐行同构的同一份实现，且它们的真差异只在构造参数上」。
//! 历史上这三份都是先在两个宿主里各写一遍、发现连注释和单测都同名同义之后才合并的：
//!
//! | 实现 | 端口 | 宿主之间的真差异 |
//! |---|---|---|
//! | [`JsonFileIdentityStore`] | `KeychainProvider` + `PairedDeviceStore` | 目录从哪来 |
//! | [`JsonFileDeviceConfig`] | `DeviceConfig` | 文件路径从哪来 |
//! | [`LocalFileAccess`] | `FileAccess` | 保存位置从哪来 |
//!
//! 反过来，**平台细节不得为了下沉而塞进来**：落点语义不同的宿主（系统文档提供方、
//! 浏览器存储）自有实现，不在此列。
//!
//! ## 为什么 core 不依赖它
//!
//! [`swarmdrop_core`](https://docs.rs/swarmdrop-core) 要过 wasm 双 target 门禁，而这里
//! 全是 native-only 的实现。依赖方向因此是：core → 端口；**宿主 → 端口 + 实现**。
//! 谁用实现谁声明它，依赖图上一眼看得出。

pub mod device_config_file;
pub mod identity_store_file;
pub mod local_fs;

pub use device_config_file::JsonFileDeviceConfig;
pub use identity_store_file::JsonFileIdentityStore;
pub use local_fs::LocalFileAccess;
