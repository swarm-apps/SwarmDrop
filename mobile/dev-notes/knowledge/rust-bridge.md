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
```

它们刷新 Rust 静态库、TS bindings 和 C++ bridge，**并在末尾链一次 `bob build`**（`&& bob build`
写在四个 `build:*` 脚本里，不必再手动补 `prepare`）。

### 为什么必须链 `bob build`：产物三层，错序就是启动即崩

这层桥有三份产物，运行时校验只认最后一份：

| 产物 | 谁生成 | 谁消费 |
|---|---|---|
| `src/generated/*.ts` + `cpp/generated/*` | `ubrn ... --and-generate` | 下面两者的输入 |
| `jniLibs/**/*.a`（Rust 静态库，含真实 checksum 函数） | 同上 | CMake 链进 `.so` |
| `lib/module/*.js` | `bob build`（= `prepare`） | **Metro 实际加载的就是它**（`exports.default`） |

`lib/module` 里编着一串 `uniffiEnsureInitialized()` 校验常量。它若落后于 `.a`，App 一进
`import` 就抛：

```
FFI function uniffi_..._checksum_method_mobilecore_revoke_pair_invite
has a checksum mismatch; this may signify previously undetected incompatible Uniffi versions
```

**这不是 uniffi 版本问题**（错误文案会把人带偏），是同一版本里 JS 与原生库不同代。

`prepare` 挂在 `pnpm install` 上，所以默认顺序恰好是**反的**：install 先用仓库里提交的
`src/generated` 编出 `lib/`，之后 `--and-generate` 才把 `src/generated` 覆盖成新的 ——
`lib/` 就此停在上一代。

**发版流水线踩过一次（`mobile-v0.10.0`）**：CI 正是 `pnpm install` → `build:android:release`
→ gradle 打包，且当时重新生成的 bindings **没提交进仓库**，于是 `lib/` 编自旧的提交版本，
打出来的 APK 一启动就崩。CI 全绿 —— 它只验证「编得过」，从不启动 App。所以：

- **`lib/` 是 gitignore 的**（`packages/*/lib/`），CI 每次现编，因此**提交的 `src/generated`
  必须是最新的**，否则 CI 那次 `bob build` 拿的就是旧输入；
- 但即使有人又忘了提交，`&& bob build` 也会在 generate 之后重编 `lib/`，把这条失败模式
  从根上掐掉。两道保险都留着。

### 没有 Xcode / NDK 时怎么重生成 bindings

改了 `#[uniffi::export]` 的签名但手边跑不了 `build:ios` / `build:android`（缺 Xcode 或
NDK）时，不要手改 `src/generated`——它是生成物，手改必然与 `.a` 里的 checksum 对不上。
用 ubrn 的 generate 子命令，它只需要一个**已编好的 cdylib**：

```bash
cargo build -p swarmdrop-mobile-core     # 产出 target/debug/libswarmdrop_mobile_core.dylib
cd mobile/packages/swarmdrop-core/rust/mobile-core   # ⚠️ 必须在 Cargo.toml 所在目录跑
../../../../node_modules/.bin/ubrn generate jsi bindings \
  --library ../../../../../target/debug/libswarmdrop_mobile_core.dylib \
  --ts-dir  ../../src/generated \
  --cpp-dir ../../cpp/generated
cd ../.. && ../../node_modules/.bin/bob build          # 刷新 lib/，见上一节
```

三个易踩点：

- **cwd 必须是 rust crate 目录**。ubrn 会就地跑 `cargo metadata`，在别处跑只会报
  `manifest path 'Cargo.toml' does not exist`——那条信息不会提示你 cd 去哪。
- **`bob build` 不能省**。`pnpm typecheck` 与 Metro 看的都是 `lib/`（`lib/typescript/src/generated/*.d.ts`），
  只重生成 `src/generated` 的话新签名根本不进 app 的类型视图，表现是「明明改了返回值，
  tsc 还说是 void」。
- 环境里没有 prettier / clang-format 时它会打印 `Skipping formatting…` 然后照常生成。
  仓库里现存的产物本来就是未格式化的，所以这不会造成整文件 diff。

`generate jsi bindings` **只**生成 TS + C++ bindings，不碰 turbo-module 胶水与
`jniLibs/*.a`。真机产物仍需一次完整 `build:ios` / `build:android`。

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

### `Option<T>` 在 uniffi 与 specta 下生成的 TS 类型不一样

同一个 Rust `Option<String>`（现成的例子：`InboxItemSummary.root_path`、`last_opened_at`）：

| codegen | 消费端 | 生成的 TS |
|---|---|---|
| **uniffi**（移动） | `src/generated/*.ts` | `field?: string` —— 可选属性，缺席是 `undefined` |
| **specta**（桌面 / Web） | `src/lib/bindings.ts`、`crates/web/bindings/bindings.ts` | `field: string \| null` |

所以**跨端共用的展示逻辑签名不能照抄桌面的 `string | null`**——移动端传进来的是
`string | undefined`，`tsc` 会直接报 TS2345。移动端这一侧写 `string | undefined` 即可
（调用点全是 uniffi 生成的类型）；只有当同一个函数要同时吃两套 codegen 的产物时，
才需要 `string | null | undefined` 并用 falsy 判别覆盖两种缺席。

这不是可以统一掉的东西：两套 codegen 对 `Option<T>` 的映射约定本来就不同，改任一边都是
和上游打架。**留意点在「抄」上**：从桌面的 hook / 工具函数复制一份到 `mobile/src` 时，
类型签名往往是最后才暴露问题的地方（逻辑照抄能跑，`?? ""` 也照样工作）。

记于 2026-08-05（收件箱标题结构化那次改动的副产物）。

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

### `UniffiError` 的 message 与 inner **都不能给用户看**（2026-08-05 修正）

ubrn 的 `UniffiError` 只把 `EnumName.Variant` 塞进 `message`，payload 在 `.inner` 数组
（uniffi enum variant 的关联字段）。

> **本条推翻了旧版规则。** 旧文写的是「用 `errorMessage()` helper 展开 inner」，把展开后的
> 串当成给用户看的文案 —— 那正是问题本身：展开出来的是 **Rust 侧写的中文技术描述**
> （`FfiError.Transfer: 收件箱条目不存在`），英文界面上原样弹出。当时有 20 处 `toast.error`
> 这么用。`errorMessage()` 已从 `lib/utils.ts` 删除。

`err.tag` 才是**稳定的语言无关判别码**（与 Rust `AppError` 的变体名一一对应）。规则两条：

- **用户文案** → `getErrorMessage(err)`：查 `KIND_MESSAGES` 表拿 Lingui 描述符，
  未命中的 kind 落通用兜底。桌面 `src/lib/errors.ts` 是同构的另一份。
- **技术细节** → 只进 `console`。`getErrorMessage` 内部已经 log 了一份，调用点不必重复；
  需要单独取时用 `errorDetail(err)`。

**判别某个 kind 也用 `tag`，不要对 message 做子串匹配。** `event-bus.ts` 曾写
`msg.includes("NodeNotStarted")` 来静默节点切换窗口期的预期错误 —— Rust 那句话改一个字
这条静默就失效，表现是切换节点时冒出一串无害的 warn。现在是 `isErrorKind(err, "NodeNotStarted")`。

**加了新 kind 要做两件事**：Rust 侧 `FfiError` 补变体 + 双向映射，然后**重新生成绑定**
（`FfiError_Tags` 是生成物）；JS 侧在 `KIND_MESSAGES` 加一条并补 en 译文。
漏掉后者不会报错，只会静默落到「出错了，请重试」。

**相关文件**：[src/lib/errors.ts](../../src/lib/errors.ts),
[src/core/event-bus.ts](../../src/core/event-bus.ts)

### 会话失败原因也是判别码：`projection.failure`，别再猜 `errorMessage`（2026-08-05）

上一条讲的是**命令返回值**（`FfiError`）。会话级失败原因走的是**另一条通道**
——`TransferProjection` 的一个字段，落在 DB 里、不进 wire。它同样已经判别码化：
`MobileFailureCode`（`FileFinalizeFailed` / `SessionExpired` / `ResumeRejected` /
`OfferFailed` / `Legacy`），渲染走 `failureCodeLabel()` 的穷尽 `switch`。

**这里曾经是 `friendlyTransferError` 的 9 条英文关键词正则，而且它会误命中。**
输入是 `format!("文件最终化失败: {name} (file_id={id}): {e}")` —— **文件名拼在串里**，
正则对整串跑：

| 文件叫 | 命中 | 显示 | 真实原因 |
|---|---|---|---|
| `Q3-cancel.xlsx` | `/(cancel\|abort)/` | 「传输已取消」 | 校验失败 |
| `network-diagram.png` | `/(network\|connect…)/` | 「网络连接中断」 | 校验失败 |

确定性复现，不是概率。**「传输已取消」把一次数据损坏说成用户自己的操作** —— 比看到
兜底文案糟得多。这也是「别对自由文本做匹配」这条规则的第二次翻车（第一次见上条的
`event-bus.ts`），区别在于上一次是**静默失效**，这次是**自信地给错答案**。

`Legacy { message }` 是判别码引入之前落库的老会话，原样展示；**不写回填** ——
失败原因的原始错误早已不存在，重算不出来。（收件箱标题相反，那边**回填**了，
因为标题能从文件列表重算。）

**加新 code 要三步**：`crates/transfer/src/failure.rs` 加变体 → `history.rs` 的
`MobileFailureCode` 镜像跟上（`From` impl 是穷尽 match，漏了编译不过）→ 重新生成绑定
→ `failureCodeLabel` 补分支（穷尽 switch，漏了 tsc 会红）。三道门都是编译期的，
比 `KIND_MESSAGES` 那条安全。

**相关文件**：[src/components/transfer/shared.tsx](../../src/components/transfer/shared.tsx)、
`crates/transfer/src/failure.rs`

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

### Android 14+ FGS connectedDevice 需要「前置权限」,缺了 startForeground 直接崩进程

manifest 声明 `foregroundServiceType="connectedDevice"` + `FOREGROUND_SERVICE_CONNECTED_DEVICE`
**不够**:Android 14+ 在 `startForeground()` 时还检查该 type 的前置权限集(anyOf:
`CHANGE_NETWORK_STATE` / `CHANGE_WIFI_STATE` / `CHANGE_WIFI_MULTICAST_STATE` / NFC /
TRANSMIT_IR / 蓝牙运行时权限 / UWB)。一个都没有 → SecurityException 抛在 Java service
生命周期内,**JS 侧 try/catch 拦不住,进程闪退**(vivo 真机「启动节点就闪退」根因,v0.7.1 修复)。

**正确做法**:
- `plugins/with-android-foreground-service.js` 注入 `CHANGE_NETWORK_STATE` + `CHANGE_WIFI_STATE`
  (normal 权限,声明即授予,无运行时弹窗)。
- 验证要在 **API 34+ 设备/模拟器 + 完整 prebuild 产物**上做:`dumpsys activity services <pkg>`
  看 `isForeground=true types=0x00000010`(0x10=CONNECTED_DEVICE)。

**不要做**:
- 别只在旧 prebuild 的 debug 包上验证——本地 android/ 落后于 plugin 时,FGS 静默失败
  (JS catch 吞成 console.warn),前台跑着节点完全无感,真机 release 包才暴露崩溃。

**相关文件**:`plugins/with-android-foreground-service.js`,`src/core/foreground-service.ts`
