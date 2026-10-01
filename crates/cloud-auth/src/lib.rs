//! 云账户的授权与凭证生命周期；只有短期访问凭证可离开本 crate。
mod account;
mod credentials;
mod error;
mod google;
mod manager;
mod provider;
mod store;

pub use account::{AccountSnapshot, AccountStatus, AccountUpdate};
pub use credentials::{AccessTokenLease, AccountCredentials, ClientCredentials, TokenMaterial};
pub use error::CloudAuthError;
pub use google::GoogleProvider;
pub use manager::CloudAccountManager;
pub use provider::{
    CloudAccountProvider, ConnectSession, PendingConnect, ProviderId, RevokeOutcome,
};
pub use store::{CredentialStore, JsonCredentialStore};

#[cfg(test)]
mod tests;
