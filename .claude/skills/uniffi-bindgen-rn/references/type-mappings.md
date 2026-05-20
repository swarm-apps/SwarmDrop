# uniffi-bindgen-rn: 类型映射 Rust ↔ TypeScript

## 标量类型

| Rust | TypeScript | 备注 |
|------|-----------|------|
| `u8`, `u16`, `u32`, `i8`, `i16`, `i32` | `number` | |
| `f32`, `f64` | `number` | |
| `u64`, `i64` | `bigint` | 64 位用 BigInt |
| `bool` | `boolean` | |
| `String` | `string` | UTF-8 |

## 容器类型

| Rust | TypeScript | 备注 |
|------|-----------|------|
| `Vec<u8>` | `ArrayBuffer` | 高效二进制传输 |
| `Vec<T>` | `Array<T>` | |
| `HashMap<K, V>` | `Map<K, V>` | |
| `Option<T>` | `T \| undefined` | |
| `SystemTime` | `Date` | |
| `Duration` | `number` | 毫秒 |

## Records（值类型，无方法，按值传递）

```rust
#[derive(uniffi::Record)]
struct DocMeta {
    doc_id: String,
    title: String,
    #[uniffi(default = 0)]
    lamport_clock: u64,
}
```

```typescript
type DocMeta = { docId: string; title: string; lamportClock: bigint };
// 工厂函数处理默认值
const meta = DocMeta.create({ docId: "abc", title: "My Note" });
```

> 命名自动转换：Rust `snake_case` → TS `camelCase`

## Objects（引用类型，有方法，按引用传递）

```rust
#[derive(uniffi::Object)]
struct NetworkManager { peers: Vec<String> }

#[uniffi::export]
impl NetworkManager {
    #[uniffi::constructor]
    fn new() -> Self { Self { peers: vec![] } }

    #[uniffi::constructor(name = "with_bootstrap")]
    fn with_bootstrap(nodes: Vec<String>) -> Self { Self { peers: nodes } }

    fn peer_count(&self) -> u32 { self.peers.len() as u32 }
}
```

```typescript
class NetworkManager {
    constructor();
    static withBootstrap(nodes: string[]): NetworkManager;
    peerCount(): number;
    uniffiDestroy(): void;  // 手动释放
}
```

**生成的接口**: `${OBJECT_NAME}Interface`，用于 mock 和参数/返回值类型。

**Trait 自动映射**: `Display` → `toString()`, `Debug` → `toDebugString()`, `Eq` → `equals()`, `Hash` → `hashCode()`

**GC 集成**: JS 对象被 GC 回收时自动 drop Rust 端的 Arc。

## Enums

### 简单枚举
```rust
#[derive(uniffi::Enum)]
enum SyncStatus { Idle, Syncing, Synced, Error }
```
```typescript
enum SyncStatus { Idle, Syncing, Synced, Error }
```

### 带数据枚举（Tagged Union）
```rust
#[derive(uniffi::Enum)]
enum SyncEvent {
    Connected,
    Progress(u32, u32),
    DocumentUpdated { doc_id: String, title: String },
    Error { message: String },
}
```
```typescript
type SyncEvent =
    | { tag: SyncEvent_Tags.Connected }
    | { tag: SyncEvent_Tags.Progress; inner: [number, number] }
    | { tag: SyncEvent_Tags.DocumentUpdated; inner: { docId: string; title: string } }
    | { tag: SyncEvent_Tags.Error; inner: { message: string } };

// 构造
const event = new SyncEvent.Progress(10, 100);
// 类型守卫
if (SyncEvent.Error.instanceOf(event)) { console.error(event.inner.message); }
```

### 带判别值
```rust
#[derive(uniffi::Enum)]
pub enum Priority { Low = 1, Medium = 2, High = 3 }
```
