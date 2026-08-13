//! 配对相关共享模型。

pub mod manager;

pub use manager::{
    PairedDeviceCommit, PairingManager, PairingPorts, PairingService, persisted_or_absent,
};
