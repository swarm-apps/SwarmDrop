# Rust Bridge (uniffi-bindgen-react-native)

## 概览

`packages/swarmdrop-core` 把 Rust `swarmdrop-core` 通过 uniffi-bindgen-react-native（ubrn）暴露
给 JS。RN 这一侧的入口在 [src/core/mobile-core.ts](../../src/core/mobile-core.ts)，文件系统
回调实现在 [src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)，事件分发在
[src/core/event-bus.ts](../../src/core/event-bus.ts)。Rust 侧的 mobile-core 在
[packages/swarmdrop-core/rust/mobile-core](../../packages/swarmdrop-core/rust/mobile-core)。

主要约束都跟"FFI 类型形状"和"panic / 错误的可见性"相关，列在下面。

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

### Sink 写入时按需 open FileHandle，不要长期持有

`ExpoFileAccess.sinks` Map 只存 `metadata + File` 引用，不缓存 `FileHandle`。每次
`writeSinkChunk` 临时 open → seek offset → write → finally close。这样跨 chunk 不会泄漏 fd，
也能容忍 app 进入后台后系统回收 handle。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)

### MobileFileMetadata.saveDir 必须由 core 注入

`ensureSinkFile` 严格要求 `metadata.saveDir` 存在；core/host.rs 在 receive 路径调
`create_sink` 时一定塞 save_dir。这避免了"全局共享 sink + None 路径"的歧义。Send 路径调
`sourceMetadata` 时 saveDir=undefined 是正常的。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)
