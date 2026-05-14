//! Desktop host adapters.

pub mod event_bus;
#[cfg(not(target_os = "android"))]
pub mod keychain;
pub mod notifier;
pub mod paths;
pub mod update_installer;
