//! 端口实现：把命令行宿主的能力接到 [`swarmdrop_host`] 的端口上。
//!
//! **本层不含业务判断**。它只回答「这个平台上，这件事怎么做」——文件怎么读、
//! 事件往哪送、数据目录在哪。什么时候该做、做了之后怎么呈现，都不属于这里。

pub mod events;
pub mod paths;
pub mod receive;

// `FileAccess` 不在这里：本地文件系统那份由 `swarmdrop-host-fs` 提供
// （`LocalFileAccess`），桌面与命令行宿主共用同一份实现。
