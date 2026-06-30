# Rust Bridge (uniffi-bindgen-react-native)

## 概览

`packages/swarmdrop-core` 把 Rust `swarmdrop-core` 通过 uniffi-bindgen-react-native（ubrn）暴露
给 JS。RN 这一侧的入口在 [src/core/mobile-core.ts](../../src/core/mobile-core.ts)，文件系统
回调实现在 [src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)，事件分发在
[src/core/event-bus.ts](../../src/core/event-bus.ts)。Rust 侧的 mobile-core 在
[packages/swarmdrop-core/rust/mobile-core](../../packages/swarmdrop-core/rust/mobile-core)。

主要约束都跟"FFI 类型形状"和"panic / 错误的可见性"相关，列在下面。

## Core revision and generated artifacts

### 更新 shared core 后必须同步 ubrn + Bob 两层产物

`packages/swarmdrop-core/rust/mobile-core/Cargo.toml` 通过 git `rev` 引用桌面仓的 shared core
crate。同步桌面端 API 时先记录目标 commit，再把 `swarmdrop-core` / `entity` / `migration` /
`swarm-p2p-core` 固定到同一个 rev，避免 Cargo 拉出两份 `swarm-p2p-core`。

UniFFI 接口变化后至少跑：

```bash
pnpm --filter react-native-swarmdrop-core build:android
pnpm --filter react-native-swarmdrop-core build:ios
pnpm --filter react-native-swarmdrop-core prepare
```

前两步刷新 Rust 静态库、TS bindings 和 C++ bridge；最后一步刷新 package `exports.types` 指向的
`lib/typescript`，否则 app 的 `react-native-swarmdrop-core` 类型解析会继续看到旧 API。

### 镜像 core struct 的 `From` impl 必须用穷尽解构（drift guard）

mobile-core 给每个跨 FFI 的 core 类型手写一个 `Mobile*` uniffi 镜像——这是 uniffi 官方推荐的
wrap 层模式：core 保持平台中立（可同时被 Tauri / RN / WASM / CLI 复用）、FFI 类型按端调优
（`PeerId/Uuid → String`、`"" → Option`）。**不要**反过来给 core 加 uniffi feature/derive：
uniffi 无法 derive `PeerId` / SeaORM `Model` / `chrono::DateTime`（仍要投影），还会污染
megazord 的类型名空间。

代价是镜像会和 core 漂移。约定：**镜像的 `From<CoreStruct>` 一律先穷尽解构再构造**，禁止用字段
访问（`offer.field`）——这样 core 给该 struct 加字段时，mobile-core 会**编译失败**而非静默漏字段：

```rust
// ✅ 穷尽解构（无 `..`）：core 加字段 → 这里编译报错，逼你处理
impl From<TransferOfferEvent> for MobileTransferOffer {
    fn from(offer: TransferOfferEvent) -> Self {
        let TransferOfferEvent { session_id, peer_id, /* …列全… */, origin } = offer;
        Self { session_id: session_id.to_string(), origin: origin.into(), /* … */ }
    }
}
```

- **enum 的 `From` 不用改**：`match` 已天然穷尽，core 加变体即编译失败。
- 只用不到的字段绑 `_`（如 `paired_at: _`），保持穷尽的同时不触发 unused 警告。

**相关文件**：`packages/swarmdrop-core/rust/mobile-core/src/{transfer,events,network,history,device,inbox,file_access}.rs`

## Callback 错误必须包成 uniffi enum 形状

### 抛错前用 `FfiError.Variant.new(msg)` 包装

ubrn 在 lift callback return 时认错误类型，如果 callback 抛了普通 `Error`，会走
`handle_callback_unexpected_error` → Rust panic（catch_unwind 后 abort），日志只剩固定字符串
`"Rust panic"`，丢失源信息。所有 `ForeignFileAccess` 方法用 `wrapFfi` 统一兜底转换成 `FfiError.Io`。

**正确做法**：

```ts
async function wrapFfi<T>(fn: () => Promise<T> | T): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw FfiError.Io.new(message);
  }
}
```

**不要做**：在 callback 里 `throw new Error(...)` —— uniffi 看不懂，会被吞成 `"Rust panic"`。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)

### 读 UniffiError.message 时必须展开 .inner

ubrn 的 `UniffiError` 只把 `EnumName.Variant` 塞进 `message`，真正的 payload 在 `.inner` 数组（
uniffi enum variant 的关联字段）。直接读 `err.message` 给用户看会显示 `"FfiError.Transfer"` 这种
没信息量的字符串。

**正确做法**：用 `errorMessage()` helper，它自动展开 inner。

```ts
const inner = (err as { inner?: unknown }).inner;
if (Array.isArray(inner) && inner.length > 0) {
  return `${err.message}: ${inner.map(String).join(", ")}`;
}
```

**相关文件**：[src/lib/utils.ts](../../src/lib/utils.ts)

## Panic 可见性

### 用 take_last_panic() 拉 Rust panic 详情

移动端没法看 logcat/oslog 时，靠 Rust 端的全局 panic hook 把 location + payload + backtrace
缓存起来。RN 端 catch 到 `"Rust panic"` 后立即调 `getMobileCore().takeLastPanic()` 拿详情打到
console / toast。Hook 是进程级，`MobileCore::new` 安装一次。

**正确做法**：

```ts
} catch (err) {
  let panicDetail: string | undefined;
  try { panicDetail = getMobileCore().takeLastPanic() ?? undefined; } catch {}
  console.error("...", err, panicDetail);
  toast.error("...", panicDetail ?? errorMessage(err));
}
```

**相关文件**：[packages/swarmdrop-core/rust/mobile-core/src/panic_hook.rs](../../packages/swarmdrop-core/rust/mobile-core/src/panic_hook.rs),
[src/app/send/select-device.tsx](../../src/app/send/select-device.tsx)

## 文件 IO

### Android content URI 必须先 copy 才能交给 expo-file-system

`expo-file-system` v55 的 `File.open()` 不支持 `content://`（会抛 "This method cannot be used
with content URIs"）。DocumentPicker / ImagePicker 调用时必须设 `copyToCacheDirectory: true`
让 expo 拷成 `file://`。iOS 同样需要拷贝（NSItemProvider 临时授权会过期）。

**正确做法**：

```ts
await DocumentPicker.getDocumentAsync({
  copyToCacheDirectory: true,
  multiple: true,
});
```

**不要做**：把原始 `content://` URI 当 `sourceId` 塞给 core——读 chunk 时会 panic。

**相关文件**：[src/core/file-access.ts](../../src/core/file-access.ts)

### Sink 写入时保持 FileHandle 打开，直到 finalize/cleanup

`ExpoFileAccess.sinks` Map 必须保存打开的 `FileHandle`。尤其是 Android SAF 的 `content://`
tree，`openFileDescriptor("w")` 在不少 DocumentsProvider 上会 truncate；如果每个 chunk 都
open/close，同一个文件会被反复截断，最后只剩尾块。正确生命周期是 create/open 阶段拿 handle，
`writeSinkChunk` 只做 seek + write，`finalizeSink` / `cleanupSink` 再 close。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)

### MobileFileMetadata.saveDir 必须由 core 注入

`ensureSinkFile` 严格要求 `metadata.saveDir` 存在；core/host.rs 在 receive 路径调
`create_sink` 时一定塞 save_dir。这避免了"全局共享 sink + None 路径"的歧义。Send 路径调
`sourceMetadata` 时 saveDir=undefined 是正常的。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)
