use crate::{
    AccessTokenLease, AccountCredentials, AccountSnapshot, AccountStatus, AccountUpdate,
    ClientCredentials, CloudAccountProvider, CloudAuthError, ConnectSession, CredentialStore,
    ProviderId, RevokeOutcome, credentials::now,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio_util::sync::CancellationToken;

struct RuntimeAccount {
    credentials: AccountCredentials,
    save_pending: bool,
    active: bool,
}
struct Account {
    snapshot: RwLock<AccountSnapshot>,
    runtime: Mutex<RuntimeAccount>,
}
struct Pending {
    account_id: String,
    cancel: CancellationToken,
}

pub struct CloudAccountManager {
    store: Arc<dyn CredentialStore>,
    providers: BTreeMap<ProviderId, Arc<dyn CloudAccountProvider>>,
    accounts: RwLock<BTreeMap<String, Arc<Account>>>,
    pending: Mutex<BTreeMap<String, Pending>>,
    updates: broadcast::Sender<AccountUpdate>,
}

impl CloudAccountManager {
    pub async fn new(
        store: Arc<dyn CredentialStore>,
        providers: Vec<Arc<dyn CloudAccountProvider>>,
    ) -> Result<Arc<Self>, CloudAuthError> {
        let providers = providers
            .into_iter()
            .map(|p| (p.provider_id(), p))
            .collect::<BTreeMap<_, _>>();
        let mut accounts = BTreeMap::new();
        for mut credentials in store.load_all().await? {
            if !providers.contains_key(&credentials.snapshot.provider) {
                return Err(CloudAuthError::InvalidConfiguration);
            }
            // 崩溃时停在刷新中并不证明远端刷新成功；只在下一次操作核验旧凭证。
            if credentials.snapshot.status == AccountStatus::Refreshing {
                credentials.snapshot.status = AccountStatus::Connected;
            }
            accounts.insert(
                credentials.snapshot.id.clone(),
                Arc::new(Account {
                    snapshot: RwLock::new(credentials.snapshot.clone()),
                    runtime: Mutex::new(RuntimeAccount {
                        credentials,
                        save_pending: false,
                        active: true,
                    }),
                }),
            );
        }
        let (updates, _) = broadcast::channel(64);
        Ok(Arc::new(Self {
            store,
            providers,
            accounts: RwLock::new(accounts),
            pending: Mutex::new(BTreeMap::new()),
            updates,
        }))
    }
    pub fn subscribe(&self) -> broadcast::Receiver<AccountUpdate> {
        self.updates.subscribe()
    }
    pub async fn list(&self) -> Vec<AccountSnapshot> {
        let accounts: Vec<_> = self.accounts.read().await.values().cloned().collect();
        let mut result = Vec::with_capacity(accounts.len());
        for account in accounts {
            result.push(account.snapshot.read().await.clone());
        }
        result
    }
    async fn account(&self, id: &str) -> Result<Arc<Account>, CloudAuthError> {
        self.accounts
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or(CloudAuthError::AccountNotFound)
    }
    pub async fn status(&self, id: &str) -> Result<AccountSnapshot, CloudAuthError> {
        Ok(self.account(id).await?.snapshot.read().await.clone())
    }
    fn provider(&self, id: ProviderId) -> Result<Arc<dyn CloudAccountProvider>, CloudAuthError> {
        self.providers
            .get(&id)
            .cloned()
            .ok_or(CloudAuthError::InvalidConfiguration)
    }

    pub async fn start_connect(
        self: &Arc<Self>,
        provider: ProviderId,
        client: ClientCredentials,
        label: String,
        reconnect_id: Option<String>,
    ) -> Result<ConnectSession, CloudAuthError> {
        client.validate()?;
        let account_id = reconnect_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if let Some(id) = &reconnect_id
            && self.status(id).await?.provider != provider
        {
            return Err(CloudAuthError::InvalidConfiguration);
        }
        let connect = self
            .provider(provider)?
            .start_connect(client.clone())
            .await?;
        let session = connect.session.clone();
        let cancel = CancellationToken::new();
        {
            let mut pending = self.pending.lock().await;
            if pending.values().any(|p| p.account_id == account_id) {
                return Err(CloudAuthError::InvalidConfiguration);
            }
            pending.insert(
                session.id.clone(),
                Pending {
                    account_id: account_id.clone(),
                    cancel: cancel.clone(),
                },
            );
        }
        let manager = self.clone();
        let session_id = session.id.clone();
        tokio::spawn(async move {
            let token = tokio::select! {
                _ = cancel.cancelled() => Err(CloudAuthError::Cancelled),
                result = connect.completion => result,
            };
            // 授权已完成后提交凭证不再被取消分支打断，避免后台原子写留下孤立账户。
            let result = match token {
                Ok(token) if !cancel.is_cancelled() => {
                    let credentials = AccountCredentials {
                        snapshot: AccountSnapshot {
                            id: account_id.clone(),
                            provider,
                            label: if label.trim().is_empty() {
                                format!("{} {}", provider.display_name(), &account_id[..8])
                            } else {
                                label
                            },
                            status: AccountStatus::Connected,
                        },
                        client,
                        token,
                    };
                    manager
                        .complete_connect(credentials, reconnect_id.is_some())
                        .await
                }
                Ok(_) => Err(CloudAuthError::Cancelled),
                Err(error) => Err(error),
            };
            manager.pending.lock().await.remove(&session_id);
            let update = match result {
                Ok(account) => AccountUpdate {
                    session_id: Some(session_id),
                    account: Some(account),
                    removed_account_id: None,
                    error: None,
                },
                Err(error) => AccountUpdate {
                    session_id: Some(session_id),
                    account: None,
                    removed_account_id: None,
                    error: Some(error),
                },
            };
            let _ = manager.updates.send(update);
        });
        Ok(session)
    }

    async fn complete_connect(
        &self,
        credentials: AccountCredentials,
        reconnect: bool,
    ) -> Result<AccountSnapshot, CloudAuthError> {
        let snapshot = credentials.snapshot.clone();
        if reconnect {
            let account = self.account(&snapshot.id).await?;
            let mut runtime = account.runtime.lock().await;
            if !runtime.active {
                return Err(CloudAuthError::Cancelled);
            }
            self.store.save(&credentials).await?;
            runtime.credentials = credentials;
            runtime.save_pending = false;
            *account.snapshot.write().await = snapshot.clone();
        } else {
            self.store.save(&credentials).await?;
            self.accounts.write().await.insert(
                snapshot.id.clone(),
                Arc::new(Account {
                    snapshot: RwLock::new(snapshot.clone()),
                    runtime: Mutex::new(RuntimeAccount {
                        credentials,
                        save_pending: false,
                        active: true,
                    }),
                }),
            );
        }
        Ok(snapshot)
    }

    pub async fn reconnect(self: &Arc<Self>, id: &str) -> Result<ConnectSession, CloudAuthError> {
        let account = self.account(id).await?;
        let credentials = account.runtime.lock().await.credentials.clone();
        self.start_connect(
            credentials.snapshot.provider,
            credentials.client,
            credentials.snapshot.label,
            Some(id.into()),
        )
        .await
    }
    pub async fn cancel_connect(&self, session_id: &str) {
        if let Some(pending) = self.pending.lock().await.get(session_id) {
            pending.cancel.cancel();
        }
    }

    pub async fn access_token(
        &self,
        id: &str,
        min_validity: Duration,
    ) -> Result<AccessTokenLease, CloudAuthError> {
        self.token(id, min_validity, None).await
    }
    /// 只刷新仍被拒绝的那一代 token，避免并发 401 导致轮换两次。
    pub async fn refresh_after_rejection(
        &self,
        id: &str,
        rejected: &AccessTokenLease,
    ) -> Result<AccessTokenLease, CloudAuthError> {
        self.token(id, Duration::ZERO, Some(rejected.secret()))
            .await
    }
    /// 只有当前凭证仍是被拒绝的那一代时，才改变账户状态。
    pub async fn require_reconnect_after_rejection(
        &self,
        id: &str,
        rejected: &AccessTokenLease,
    ) -> Result<(), CloudAuthError> {
        let account = self.account(id).await?;
        let mut runtime = account.runtime.lock().await;
        if runtime.active && runtime.credentials.token.access_token == rejected.secret() {
            self.set_status(&account, &mut runtime, AccountStatus::ReconnectRequired)
                .await;
            self.store.save(&runtime.credentials).await?;
        }
        Ok(())
    }
    async fn token(
        &self,
        id: &str,
        min_validity: Duration,
        rejected: Option<&str>,
    ) -> Result<AccessTokenLease, CloudAuthError> {
        let account = self.account(id).await?;
        let mut runtime = account.runtime.lock().await;
        if !runtime.active {
            return Err(CloudAuthError::AccountNotFound);
        }
        if runtime.credentials.snapshot.status == AccountStatus::ReconnectRequired {
            return Err(CloudAuthError::ReconnectRequired);
        }
        if runtime.save_pending {
            let mut saved = runtime.credentials.clone();
            saved.snapshot.status = AccountStatus::Connected;
            self.store.save(&saved).await?;
            runtime.credentials = saved;
            runtime.save_pending = false;
            self.set_status(&account, &mut runtime, AccountStatus::Connected)
                .await;
        }
        let current = &runtime.credentials.token;
        let expired = current.expires_at <= now().saturating_add(min_validity.as_secs());
        if !expired && rejected != Some(current.access_token.as_str()) {
            return Ok(AccessTokenLease::new(current));
        }
        self.set_status(&account, &mut runtime, AccountStatus::Refreshing)
            .await;
        let result = self
            .provider(runtime.credentials.snapshot.provider)?
            .refresh(&runtime.credentials.client, &runtime.credentials.token)
            .await;
        match result {
            Ok(token) => {
                // 新值即使未落盘也留在锁内存里，后续只重试保存，不再用作废旧值刷新。
                runtime.credentials.token = token;
                runtime.credentials.snapshot.status = AccountStatus::Connected;
                if self.store.save(&runtime.credentials).await.is_err() {
                    runtime.save_pending = true;
                    self.set_status(&account, &mut runtime, AccountStatus::SaveFailed)
                        .await;
                    return Err(CloudAuthError::SaveFailed);
                }
                self.set_status(&account, &mut runtime, AccountStatus::Connected)
                    .await;
                Ok(AccessTokenLease::new(&runtime.credentials.token))
            }
            Err(error) => {
                tracing::warn!(account_id = id, provider = runtime.credentials.snapshot.provider.as_str(), error = ?error, "云账户刷新失败");
                let status = if error == CloudAuthError::ReconnectRequired {
                    AccountStatus::ReconnectRequired
                } else {
                    AccountStatus::Connected
                };
                self.set_status(&account, &mut runtime, status).await;
                if status == AccountStatus::ReconnectRequired {
                    self.store.save(&runtime.credentials).await?;
                }
                Err(error)
            }
        }
    }
    async fn set_status(
        &self,
        account: &Account,
        runtime: &mut RuntimeAccount,
        status: AccountStatus,
    ) {
        runtime.credentials.snapshot.status = status;
        let snapshot = runtime.credentials.snapshot.clone();
        *account.snapshot.write().await = snapshot.clone();
        let _ = self.updates.send(AccountUpdate {
            session_id: None,
            account: Some(snapshot),
            removed_account_id: None,
            error: None,
        });
    }
    pub async fn disconnect(&self, id: &str) -> Result<RevokeOutcome, CloudAuthError> {
        let account = self.account(id).await?;
        let mut runtime = account.runtime.lock().await;
        for pending in self
            .pending
            .lock()
            .await
            .values()
            .filter(|p| p.account_id == id)
        {
            pending.cancel.cancel();
        }
        let outcome = self
            .provider(runtime.credentials.snapshot.provider)?
            .revoke(&runtime.credentials.token)
            .await;
        self.store
            .delete(runtime.credentials.snapshot.provider, id)
            .await?;
        runtime.active = false;
        self.accounts.write().await.remove(id);
        let _ = self.updates.send(AccountUpdate {
            session_id: None,
            account: None,
            removed_account_id: Some(id.into()),
            error: None,
        });
        Ok(outcome)
    }
}
