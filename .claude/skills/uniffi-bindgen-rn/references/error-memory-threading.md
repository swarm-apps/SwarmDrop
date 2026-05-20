# uniffi-bindgen-rn: 错误处理、内存管理、线程模型

## 错误处理

### 基本模式（Result → Promise reject）

```rust
#[derive(uniffi::Error)]
pub enum DatabaseError {
    NotFound,
    ConnectionFailed,
    CorruptedData { details: String },
}

#[uniffi::export]
fn get_document(doc_id: String) -> Result<DocMeta, DatabaseError> { /* ... */ }
```

```typescript
try {
    const doc = getDocument("abc");
} catch (e: any) {
    // ⚠️ 必须用 instanceOf() 静态方法，不能用 instanceof
    if (DatabaseError.instanceOf(e)) {
        if (DatabaseError.NotFound.instanceOf(e)) { /* ... */ }
        if (DatabaseError.CorruptedData.instanceOf(e)) {
            console.error(e.inner.details);
        }
    }
}
```

### Flat Error（只需消息）

```rust
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum AppError {
    #[error("网络不可用")]
    NetworkUnavailable,
    #[error("解析失败：行 {line}, 列 {col}")]
    ParseError { line: usize, col: usize },
}
```

JS 端收到普通 Error，message 是 `#[error("...")]` 的文本。

### 命名冲突

Rust 错误类型名为 `Error` 时，TS 自动改名为 `Exception`（避免与内置 Error 冲突）。

## 内存管理

### 自动模式（默认）

JS 对象被 GC 回收 → C++ 析构 → Rust drop(Arc<T>)。GC 时机不可预测。

### 手动释放

```typescript
const manager = new NetworkManager();
try {
    await manager.sync();
} finally {
    manager.uniffiDestroy(); // 立即释放 Rust 资源
}
```

### uniffiUse（自动 RAII 风格）

```typescript
const result = new NetworkManager().uniffiUse((manager) => {
    manager.addPeer("peer-1");
    return manager.peerCount();
});
// manager 自动销毁
```

### 实践原则

- 持有昂贵资源（网络连接、文件句柄）→ 用 `uniffiDestroy()` / `uniffiUse()`
- 纯数据/计算对象 → 依赖 GC 自动回收

## 线程模型

### 基本规则

- JS 单线程，uniffi 尊重此约束
- Rust 端可多线程（tokio spawn）
- 从 Rust 后台线程调用 JS callback → **Rust 线程阻塞等待 callback 返回**

### 死锁风险

场景：Rust 持有 Mutex → 调用 JS callback → callback 触发另一个 Rust 函数 → 该函数获取同一 Mutex → 死锁

```rust
// ❌ 错误：持有锁时调用 callback
fn notify(&self, listener: &dyn EventListener) {
    let data = self.state.lock().unwrap();
    listener.on_change(data.clone()); // 死锁风险！
}

// ✅ 正确：释放锁后调用
fn notify(&self, listener: &dyn EventListener) {
    let data = {
        let guard = self.state.lock().unwrap();
        guard.clone()
    }; // 锁已释放
    listener.on_change(data); // 安全
}
```

### WASM 单线程

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait MyAsyncTrait {
    async fn do_work(&self) -> String;
}
```
