# uniffi-bindgen-rn: SwarmNote 桥接设计

## 架构

```
共享层 (Rust):
  swarm-p2p-core → app-core ← yrs-blocknote

桌面端: app-core → src-tauri (#[tauri::command]) → React (invoke())
移动端: app-core → uniffi 绑定 (自动生成 TS + C++) → React Native (直接函数调用)
```

## app-core API 设计示例

```rust
// crates/app-core/src/lib.rs

#[derive(uniffi::Record)]
pub struct DocInfo {
    pub doc_id: String,
    pub title: String,
    pub content_preview: String,
    pub updated_at: SystemTime,
}

#[derive(uniffi::Enum)]
pub enum NetworkStatus {
    Offline,
    Connecting,
    Online { peer_count: u32 },
}

#[uniffi::export(callback_interface)]
pub trait AppEventListener: Send + Sync {
    fn on_network_status_changed(&self, status: NetworkStatus);
    fn on_peer_connected(&self, peer_id: String, device_name: String);
    fn on_sync_progress(&self, completed: u32, total: u32);
    fn on_document_updated(&self, doc_id: String);
    fn on_pairing_request(&self, peer_id: String, device_name: String);
}

#[derive(uniffi::Object)]
pub struct SwarmNoteCore { /* ... */ }

#[uniffi::export]
impl SwarmNoteCore {
    #[uniffi::constructor]
    pub async fn new(
        data_dir: String,
        listener: Box<dyn AppEventListener>,
    ) -> Self { /* ... */ }

    pub async fn start_network(&self) -> Result<(), AppError> { /* ... */ }
    pub async fn stop_network(&self) { /* ... */ }
    pub async fn list_documents(&self) -> Vec<DocInfo> { /* ... */ }
    pub async fn get_document(&self, doc_id: String) -> Result<Vec<u8>, AppError> { /* ... */ }
    pub async fn save_document(&self, doc_id: String, content: Vec<u8>) -> Result<(), AppError> { /* ... */ }
    pub async fn generate_pairing_code(&self) -> Result<String, AppError> { /* ... */ }
    pub async fn accept_pairing(&self, peer_id: String) -> Result<(), AppError> { /* ... */ }
}
```

## Tauri → UniFFI 映射对照

| Tauri (桌面端) | UniFFI (app-core) | 变化 |
|---|---|---|
| `#[tauri::command] async fn start(app: AppHandle, ...)` | `#[uniffi::export] pub async fn start_network(&self)` | 去掉 AppHandle |
| `app.emit("peer-connected", payload)` | `listener.on_peer_connected(peer_id, name)` | 事件 → callback |
| `app.state::<DbState>()` | `self.db: Arc<DatabaseConnection>` | 状态自持有 |
| `invoke("get_documents", {})` | `core.listDocuments()` | 类型安全直调 |

## RN 端使用示例

```typescript
import { SwarmNoteCore, type AppEventListener, type NetworkStatus } from 'swarmnote-rust-lib';

class SwarmNoteListener implements AppEventListener {
    constructor(private store: ReturnType<typeof useNetworkStore.getState>) {}

    onNetworkStatusChanged(status: NetworkStatus): void {
        this.store.setNetworkStatus(status);
    }
    onPeerConnected(peerId: string, deviceName: string): void {
        this.store.addPeer({ peerId, deviceName });
    }
    onSyncProgress(completed: number, total: number): void {
        this.store.setSyncProgress({ completed, total });
    }
    onDocumentUpdated(docId: string): void {
        queryClient.invalidateQueries(['documents']);
    }
    onPairingRequest(peerId: string, deviceName: string): void {
        Alert.alert(`设备 "${deviceName}" 请求配对`, '是否接受？', [
            { text: '拒绝', style: 'cancel' },
            { text: '接受', onPress: () => core.acceptPairing(peerId) },
        ]);
    }
}

const core = await SwarmNoteCore.new(dataDir, new SwarmNoteListener(store));
await core.startNetwork();
```
