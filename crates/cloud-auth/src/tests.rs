use crate::*;
use async_trait::async_trait;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Notify};

fn credentials() -> AccountCredentials {
    AccountCredentials {
        snapshot: AccountSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            provider: ProviderId::GoogleDrive,
            label: "测试账户".into(),
            status: AccountStatus::Connected,
        },
        client: ClientCredentials {
            client_id: "test-client".into(),
            client_secret: "secret-sentinel".into(),
        },
        token: TokenMaterial {
            access_token: "access-sentinel".into(),
            refresh_token: "refresh-sentinel".into(),
            expires_at: 0,
        },
    }
}

#[tokio::test]
async fn store_replaces_one_account_without_touching_another_and_removes_stale_temps() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonCredentialStore::new(dir.path().into());
    let mut a = credentials();
    let b = credentials();
    store.save(&a).await.unwrap();
    store.save(&b).await.unwrap();
    let root = dir.path().join("cloud-accounts/google-drive");
    let b_path = root.join(format!("{}.json", b.snapshot.id));
    let b_before = std::fs::read(&b_path).unwrap();
    let b_time = std::fs::metadata(&b_path).unwrap().modified().unwrap();
    a.token.refresh_token = "rotated-sentinel".into();
    store.save(&a).await.unwrap();
    assert_eq!(std::fs::read(&b_path).unwrap(), b_before);
    assert_eq!(
        std::fs::metadata(&b_path).unwrap().modified().unwrap(),
        b_time
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.join(format!("{}.json", a.snapshot.id)))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let stale = root.join(format!(".{}-stale", a.snapshot.id));
    std::fs::write(&stale, "partial").unwrap();
    assert_eq!(store.load_all().await.unwrap().len(), 2);
    store
        .delete(a.snapshot.provider, &a.snapshot.id)
        .await
        .unwrap();
    assert!(!stale.exists());
    assert_eq!(store.load_all().await.unwrap().len(), 1);
}

#[tokio::test]
async fn corrupt_store_does_not_turn_into_an_empty_account_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonCredentialStore::new(dir.path().into());
    let a = credentials();
    store.save(&a).await.unwrap();
    std::fs::write(
        dir.path().join(format!(
            "cloud-accounts/google-drive/{}.json",
            a.snapshot.id
        )),
        "{partial",
    )
    .unwrap();
    assert!(matches!(
        store.load_all().await,
        Err(CloudAuthError::ReadFailed)
    ));
}

struct MemoryStore {
    data: Mutex<Vec<AccountCredentials>>,
    fail: AtomicBool,
}
#[async_trait]
impl CredentialStore for MemoryStore {
    async fn load_all(&self) -> Result<Vec<AccountCredentials>, CloudAuthError> {
        Ok(self.data.lock().await.clone())
    }
    async fn save(&self, a: &AccountCredentials) -> Result<(), CloudAuthError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(CloudAuthError::SaveFailed);
        }
        let mut data = self.data.lock().await;
        data.retain(|b| b.snapshot.id != a.snapshot.id);
        data.push(a.clone());
        Ok(())
    }
    async fn delete(&self, _: ProviderId, id: &str) -> Result<(), CloudAuthError> {
        self.data.lock().await.retain(|a| a.snapshot.id != id);
        Ok(())
    }
}
struct RotatingProvider {
    calls: AtomicUsize,
    entered: Notify,
    release: Notify,
    gated: AtomicBool,
    rejected: AtomicBool,
}
#[async_trait]
impl CloudAccountProvider for RotatingProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::GoogleDrive
    }
    async fn start_connect(&self, _: ClientCredentials) -> Result<PendingConnect, CloudAuthError> {
        Err(CloudAuthError::Denied)
    }
    async fn refresh(
        &self,
        _: &ClientCredentials,
        _: &TokenMaterial,
    ) -> Result<TokenMaterial, CloudAuthError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.gated.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        if self.rejected.load(Ordering::SeqCst) {
            return Err(CloudAuthError::ReconnectRequired);
        }
        Ok(TokenMaterial {
            access_token: "new-access-sentinel".into(),
            refresh_token: "new-refresh-sentinel".into(),
            expires_at: crate::credentials::now() + 3600,
        })
    }
    async fn revoke(&self, _: &TokenMaterial) -> RevokeOutcome {
        RevokeOutcome { revoked: false }
    }
}
async fn fixture(
    gated: bool,
) -> (
    Arc<CloudAccountManager>,
    Arc<MemoryStore>,
    Arc<RotatingProvider>,
    String,
) {
    let a = credentials();
    let id = a.snapshot.id.clone();
    let store = Arc::new(MemoryStore {
        data: Mutex::new(vec![a]),
        fail: AtomicBool::new(false),
    });
    let provider = Arc::new(RotatingProvider {
        calls: AtomicUsize::new(0),
        entered: Notify::new(),
        release: Notify::new(),
        gated: AtomicBool::new(gated),
        rejected: AtomicBool::new(false),
    });
    let manager = CloudAccountManager::new(store.clone(), vec![provider.clone()])
        .await
        .unwrap();
    (manager, store, provider, id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_refreshes_wait_for_one_persisted_generation() {
    let (manager, store, provider, id) = fixture(true).await;
    let first = {
        let manager = manager.clone();
        let id = id.clone();
        tokio::spawn(async move { manager.access_token(&id, Duration::from_secs(60)).await })
    };
    provider.entered.notified().await;
    assert_eq!(
        manager.status(&id).await.unwrap().status,
        AccountStatus::Refreshing
    );
    let second = {
        let manager = manager.clone();
        let id = id.clone();
        tokio::spawn(async move { manager.access_token(&id, Duration::from_secs(60)).await })
    };
    provider.release.notify_one();
    let a = first.await.unwrap().unwrap();
    let b = second.await.unwrap().unwrap();
    assert_eq!(a.secret(), b.secret());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.data.lock().await[0].token.refresh_token,
        "new-refresh-sentinel"
    );
    assert!(!format!("{a:?}").contains(a.secret()));
}

#[tokio::test]
async fn failed_rotation_save_pauses_operations_and_retries_save_with_new_token() {
    let (manager, store, provider, id) = fixture(false).await;
    store.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        manager.access_token(&id, Duration::ZERO).await,
        Err(CloudAuthError::SaveFailed)
    ));
    assert_eq!(
        manager.status(&id).await.unwrap().status,
        AccountStatus::SaveFailed
    );
    assert_eq!(
        store.data.lock().await[0].token.refresh_token,
        "refresh-sentinel"
    );
    store.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        manager
            .access_token(&id, Duration::ZERO)
            .await
            .unwrap()
            .secret(),
        "new-access-sentinel"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    // 未落盘时退出后，重启只能拿到旧值；provider 拒绝后须明确要求重连。
}

#[tokio::test]
async fn crash_after_remote_rotation_requires_reconnect_without_discarding_client() {
    let (manager, store, provider, id) = fixture(false).await;
    store.fail.store(true, Ordering::SeqCst);
    assert!(manager.access_token(&id, Duration::ZERO).await.is_err());
    drop(manager);
    store.fail.store(false, Ordering::SeqCst);
    provider.rejected.store(true, Ordering::SeqCst);
    let manager = CloudAccountManager::new(store.clone(), vec![provider.clone()])
        .await
        .unwrap();
    assert!(matches!(
        manager.access_token(&id, Duration::ZERO).await,
        Err(CloudAuthError::ReconnectRequired)
    ));
    assert_eq!(
        manager.status(&id).await.unwrap().status,
        AccountStatus::ReconnectRequired
    );
    assert_eq!(store.data.lock().await[0].client.client_id, "test-client");
    assert!(matches!(
        manager.access_token(&id, Duration::ZERO).await,
        Err(CloudAuthError::ReconnectRequired)
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn revoke_failure_still_removes_local_account_and_updates_never_contain_credentials() {
    let (manager, store, _, id) = fixture(false).await;
    let mut events = manager.subscribe();
    manager.access_token(&id, Duration::ZERO).await.unwrap();
    let event = events.recv().await.unwrap();
    let json = serde_json::to_string(&event).unwrap();
    assert!(!json.contains("sentinel"));
    assert!(!manager.disconnect(&id).await.unwrap().revoked);
    assert!(manager.list().await.is_empty());
    assert!(store.data.lock().await.is_empty());
}

#[tokio::test]
async fn refresh_failure_log_contains_only_account_and_safe_error_category() {
    struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let output = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(move || Capture(sink.clone()))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let (manager, _, provider, id) = fixture(false).await;
    provider.rejected.store(true, Ordering::SeqCst);
    let _ = manager.access_token(&id, Duration::ZERO).await;
    let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert!(log.contains(&id));
    assert!(log.contains("ReconnectRequired"));
    assert!(!log.contains("sentinel"));
}
