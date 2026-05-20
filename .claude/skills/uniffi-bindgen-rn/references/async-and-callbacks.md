# uniffi-bindgen-rn: 异步与回调

## async fn → Promise

```rust
#[uniffi::export]
pub async fn sync_workspace(workspace_id: String) -> Vec<DocMeta> {
    let docs = fetch_remote_docs(&workspace_id).await;
    merge_local_docs(docs).await
}
```
```typescript
const docs = await syncWorkspace("my-workspace-uuid");
```

## AbortSignal 取消（每个 async 函数自动支持）

```typescript
const controller = new AbortController();
setTimeout(() => controller.abort(), 10_000);

try {
    const result = await syncWorkspace("id", { signal: controller.signal });
} catch (e) {
    if (e instanceof Error && e.name === "AbortError") {
        console.log("已取消");
    }
}
```

取消时 Rust Future 被 drop，所有资源安全释放。

**限制**:
- ✅ async fn 作为顶层函数和对象方法
- ✅ Result 返回值（rejected Promise 带类型化错误）
- ❌ 不支持 Promise/Future 作为参数
- ❌ 不支持 Promise/Future 作为错误类型

## Callback Interface（Rust → JS 事件推送）

```rust
#[uniffi::export(callback_interface)]
pub trait SyncEventListener: Send + Sync {
    fn on_peer_connected(&self, peer_id: String, device_name: String);
    fn on_sync_progress(&self, doc_id: String, completed: u32, total: u32);
    fn on_sync_completed(&self);
    fn on_error(&self, message: String);
}

#[uniffi::export]
pub async fn start_sync(
    workspace_id: String,
    listener: Box<dyn SyncEventListener>,
) { /* ... */ }
```

```typescript
class MySyncListener implements SyncEventListener {
    onPeerConnected(peerId: string, deviceName: string): void { /* 更新 UI */ }
    onSyncProgress(docId: string, completed: number, total: number): void { /* 进度条 */ }
    onSyncCompleted(): void { /* 完成提示 */ }
    onError(message: string): void { /* 错误处理 */ }
}
await startSync("workspace-uuid", new MySyncListener());
```

### 带返回值的回调

```rust
#[uniffi::export(callback_interface)]
pub trait ConflictResolver: Send + Sync {
    fn resolve_conflict(&self, doc_id: String, local: String, remote: String) -> bool;
}
```

### 带错误处理的回调

```rust
#[derive(uniffi::Error)]
enum StorageError { DiskFull, PermissionDenied }

#[uniffi::export(callback_interface)]
pub trait StorageProvider: Send + Sync {
    fn save_data(&self, key: String, data: Vec<u8>) -> Result<(), StorageError>;
}
```
```typescript
class MyStorage implements StorageProvider {
    saveData(key: string, data: ArrayBuffer): void {
        if (full) throw new StorageError.DiskFull();
    }
}
```

## Foreign Traits（双向 trait）

用 `Arc<>` 而非 `Box<>`，Rust 和 JS 端都可以实现：

```rust
#[uniffi::export(with_foreign)]
pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, message: String);
}

#[uniffi::export]
fn set_logger(logger: Arc<dyn Logger>) { /* ... */ }
```
