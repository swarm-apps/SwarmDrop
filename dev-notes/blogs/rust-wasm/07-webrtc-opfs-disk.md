# WebRTC 流式传输 + OPFS 落盘——浏览器大文件不炸内存

> **rust-wasm 系列补篇。** 前几篇讲怎么把一份 Rust 代码编到浏览器且能跑起来；这篇讲那份
> 传输逻辑**收到大文件时把字节写去哪**。结论：浏览器收大文件不能整个塞内存，OPFS +
> 流式 positioned write 是唯一干净出路；而 SyncAccessHandle 这条「看起来更快」的岔路，
> 实测无增益、还多养一套写法，被我们主动简化掉了。这是「单核心包」在接收/落盘侧的最后一段落地。

## 结论先行

浏览器端收文件，落盘只有一个正确形状：**开一个 OPFS `createWritable` 句柄常驻整场传输，
每收到一个 256 KiB 明文块就 positioned write 直写进去，最后 `close` 提交**。

- **不能整文件缓冲进内存**——demo 初版就是攒一个大 `Blob` 再一次落盘，几百 MB 的文件直接 OOM。
- **不能用 IndexedDB / localStorage**——它们是 KV 语义，改一段要整个 value 读出改回，没有
  「seek 到 offset 写一段」这种文件系统语义；断点续传就是要这个语义。
- **SyncAccessHandle 曾被引入又被删掉**：它是 Worker-only 的同步 API，理论上每块省一次
  wasm↔JS Promise 调度；但六组基准实测吞吐无差别（瓶颈根本不在落盘），留着它只是让同一个
  端口里并存**两套写法 + 两套语义 + 一个新的 Send 边界坑**——不值，简化成单路径 `createWritable`。

这三条背后是同一个判断：**浏览器落盘的瓶颈在网络链路，不在磁盘 API 的选型;所以选那个语义最干净、
和 `Send` 纪律最好相处的写法。**

## 数据流：一块进来，一块落盘

接收侧的完整链路是「网络流 → 解密 → 逐块落盘」，落盘那一跳打在端口 trait `FileAccess` 上，
浏览器实现是 `OpfsFileAccess`：

```mermaid
graph LR
    A["WebRTC / ws 流<br/>(libp2p 传输)"] --> B["transfer receiver<br/>解密 + bao 逐块验签"]
    B -->|"每 256 KiB 一块"| C["FileAccess::write_sink_chunk<br/>(offset, data)"]
    C --> D["OpfsFileAccess<br/>positioned write 单次 Promise"]
    D --> E["OPFS createWritable 句柄<br/>(staging)"]
    E -->|"finalize → close"| F["提交落盘<br/>逐字节一致"]
    style D fill:#4dabf7,color:#fff
```

块大小是传输域的常量，两端同一份：

```rust
// crates/transfer/src/lib.rs:39-40
/// 传输分块大小：256 KiB。
pub const CHUNK_SIZE: usize = 256 * 1024;
```

接收端 actor 拿到一块明文，直接调端口写下去——**读到多少写多少，从不在业务层攒缓冲**：

```rust
// crates/transfer/src/actor/receiver.rs:429-431
self.file_access
    .write_sink_chunk(sink_id, range.offset, data)
    .await?;
```

这里关键在 `range.offset`：块是**带偏移**落盘的，不是顺序 append。这就是为什么落盘设施必须支持
positioned write——续传时后收到的块可能对应文件中段的一个 range，得能精确 seek 过去覆盖。
`write_sink_chunk` 是 `FileAccess` 端口的方法（`crates/host/src/ports.rs:146`），桌面走
`std::fs` seek+write，浏览器走 OPFS——同一个 core，两种宿主实现。这条端口边界是 transfer 域
依赖倒置的产物（[transfer-architecture/02](../transfer-architecture/02-dependency-inversion-ports.md)），
也正是「单核心包」能零 cfg 跨端的地基。

## 为什么是 OPFS，不是 IndexedDB / 内存 Blob

浏览器给过三代持久化设施,能力差一个数量级:

| 设施 | 数据模型 | positioned write | 适合 |
|---|---|---|---|
| `localStorage` | 字符串 KV,同步,~5MB | 否 | 小配置 |
| IndexedDB | 结构化对象 KV(能存 Blob) | **否**——只能整 value 读出/写回 | 元数据、小对象 |
| **OPFS** | **真·文件系统**(目录 + 文件 + 句柄) | **是** | **大文件、随机写、断点续传** |

IndexedDB 能塞 Blob，但改一个字节要把整个 value 取出改回；内存 Blob 则根本活不过大文件。
**OPFS**（Origin Private File System）是 File System Access API 的一支：每个 origin 拿到一个
私有、沙盒化的文件系统根，`navigator.storage.getDirectory()` 取得，之后就是「开文件句柄 →
seek → 写 range → close」的文件系统语义。这正是断点续传要的东西。OPFS 是什么、和 IndexedDB
的本质区别、以及它的线程约束，[browser-platform/00](../browser-platform/00-opfs.md) 讲透了，
这里只取「为什么它适合流式落盘」这一面。

## createWritable 流式写的用法

`createWritable` 返回的 `FileSystemWritableFileStream` 是 **staging 语义**：写进去的是暂存,
`close()` 才提交落盘。我们把这个句柄在 `create_sink` 时开好、塞进一张
`SendWrapper<RefCell<HashMap<FileSinkId, FileSystemWritableFileStream>>>` 常驻,整场传输复用:

```rust
// crates/web/src/opfs.rs:97-113
pub(crate) async fn open_writable(
    relative_path: &str,
    keep_existing_data: bool,
) -> AppResult<FileSystemWritableFileStream> {
    let handle = opfs_file_handle(relative_path, true).await?;
    // !Send 的 opts 只在 block 内构造并取到 Promise 后即丢，只让 SendWrapper<JsFuture> 跨 await。
    let create_promise = {
        let opts = FileSystemCreateWritableOptions::new();
        opts.set_keep_existing_data(keep_existing_data);
        handle.create_writable_with_options(&opts)
    };
    SendWrapper::new(JsFuture::from(create_promise))
        .await
        .map_err(js_to_err)?
        .dyn_into::<FileSystemWritableFileStream>()
        .map_err(|_| AppError::Transfer("createWritable 返回类型错误".into()))
}
```

`keep_existing_data` 是续传的开关：`false` 打开即截断（全新文件），`true` 保留已落盘的字节
（positioned write 只覆盖后续 range）。每块落盘用 `write_with_write_params`——WHATWG 规范的
`{ type:"write", position, data }` **一次 Promise 完成 seek+write**，别手写 `seek()` + `write()`
两次往返（热路径调度开销翻倍）:

```rust
// crates/web/src/file_access.rs:113-134
async fn write_sink_chunk(&self, sink: &FileSinkId, offset: u64, data: Vec<u8>) -> AppResult<()> {
    // positioned write：WriteParams { type:"write", position, data } 单次调用等价 seek+write，
    // 每 chunk 只走一次 JS Promise 往返。句柄与 params 在 scope 内取到 Promise 即丢。
    let promise = {
        let writable = self.sink(sink)?;
        let params = WriteParams::new(WriteCommandType::Write);
        params.set_position(Some(offset as f64));
        params.set_data(&JsValue::from(js_sys::Uint8Array::from(data.as_slice())));
        writable.write_with_write_params(&params).map_err(js_to_err)?
    };
    SendWrapper::new(JsFuture::from(promise)).await.map_err(js_to_err)?;
    Ok(())
}
```

`finalize_sink` 只需 `close` 句柄提交 staging——因为字节早就一块块直写进去了,收尾没有「刷一大坨」
的动作。取消/失败路径反而更省心:直接 drop 句柄 = 丢弃未提交的 staging,正是想要的行为。
完成后走 `export_blob_url` 读回建 blob URL 供 `<a download>` 下载。这套主线程流式写的正确姿势,
知识库 libp2p-wasm.md 有单独一节记着「不要整文件缓冲、不要两次往返、别依赖 staging 活过刷新」。

## SyncAccessHandle：一条被实测否决的岔路

OPFS 其实有第二套写 API：`createSyncAccessHandle` 返回的 `FileSystemSyncAccessHandle`,
**同步、零 Promise、写即落盘**。它是给 Worker 设计的（Window 主线程环境里根本没有这个 API）——
Worker 里可以同步阻塞,于是它把「每块一次 wasm↔JS Promise 往返」压成一次同步调用,直觉上更快。

我们**真的实现过**。提交 `ea80fdf1 feat(web): Worker 落盘升级 SyncAccessHandle` 里,
`OpfsFileAccess` 的写句柄是一个按环境二选一的枚举:

```rust
// crates/web/src/file_access.rs @ ea80fdf1（已删除）
enum SinkHandle {
    /// Window 主线程：异步 createWritable staging 流——写进暂存，close() 才提交落盘。
    Stream(FileSystemWritableFileStream),
    /// Worker：同步 SyncAccessHandle——每 chunk 零 Promise 直写落盘（无 staging），
    /// 对同一文件独占（须显式 close() 释放锁，drop 不释放）。Worker-only API。
    Sync(FileSystemSyncAccessHandle),
}
```

`open_sink_handle` 靠 `is_window()` 探针分流,写路径 `match` 两个臂,`write_chunk_promise` 对
Sync 臂同步写完返回 `None`、对 Stream 臂返回待 await 的 `Promise`。能跑,而且在 Worker 里
真的是同步直写。

**然后我们跑了基准,把它删了**(提交 `2f6a158d refactor(web): 全 crate 四路审查落地`,现在
`file_access.rs` 只剩单路径 `createWritable`,连 `env::is_window` 探针都一并移除了)。删的依据是
六组吞吐基准:

> 六组基准全 `~31±3 MB/s`——**瓶颈在 relay 链路 + noise 加解密,不在落盘方式**。
> SyncAccessHandle 每块省的那一次 Promise 调度,在 31 MB/s 的网络面前是噪声级的。

留着它要付的成本却是实打实的三笔:

| 成本 | createWritable | SyncAccessHandle |
|---|---|---|
| **锁语义** | 无独占,可重开 | **独占锁,且 drop 不释放,必须显式 close**——cleanup/取消漏 close 会把同文件重开一直挡住 |
| **提交语义** | staging,close 才落盘;取消=drop 丢弃未提交,天然干净 | 写即落盘,无 staging;取消后已写字节留盘上(语义不一致) |
| **短写** | 无 | `write_with_u8_array_and_options` 返回实际写入字节数,得**校验短写** |

再加一笔 rust-wasm 独有的税——**Send 边界坑**。二选一的枚举意味着写路径要 `match` 一个含
`!Send`（`js_sys::Promise`）的 scrutinee,而 `await` 落在 match 臂内会被 generator 判定为
scrutinee 活过整个 match:

```
error[E0277]: `*mut u8` cannot be sent between threads safely
```

要绕开得先把 `!Send` 值同步 `map` 成 `Option<SendWrapper<JsFuture>>` 再 `if let ... await`——
一套只为「双写法并存」而存在的额外纪律。**一条实测无收益的路径,却要养独占锁、不一致的取消语义、
短写校验、外加一个 Send 边界的绕法。四路审查一致判它删。** 单路径 `createWritable` 后,
`write_sink_chunk` 直白到一眼看完,没有 `match`、没有 `Option<Promise>`、没有 E0277。

## 主线程速率与卡顿:读到多少写多少

有人会担心主线程落盘会卡 UI——毕竟每块都要 `.await` 一次 JS Promise。实测结论是**不卡**,
原因就在那 `31±3 MB/s`:

- **写不是瓶颈,所以写慢一点没关系。** 一块 256 KiB 在 31 MB/s 下约 8ms 到达一块,positioned
  write 的单次 Promise 往返远快于此,落盘永远在追着网络跑、从不积压。
- **读到多少写多少,内存占用是常量。** 业务层从不攒缓冲(`write_sink_chunk` 收到块立即写、
  立即 drop),内存里同时最多一块的量级,几百 MB 的文件和几 MB 的文件占用一样。这才是「大文件不
  OOM」的真正含义——不是靠更大的 buffer,是靠**没有 buffer**。
- **staging 不等于攒内存。** `createWritable` 的 staging 是浏览器实现的暂存(可落临时文件),
  不是我们在 wasm 堆里攒;`close` 提交的开销也是一次性、与文件大小无关的元数据操作。

顺带一提,这套自动化基准还**顺手炸出了初版四道门之外的第五道门**:web-time 的 `Instant` 原点是页面导航时刻,
自动化 bench 秒开秒传,progress 的滑动窗口 `now - SPEED_WINDOW` 第一个块就 `checked_sub` 下溢
panic——人工实测时页面开了很久才点传输,永远碰不到。**时间原点类 bug 只有自动化才稳定暴露**
(初版四道门见 [wasm-debugging/](../wasm-debugging/))。所以这些吞吐数字不是白测的,它同时是压力测试。

## 与 !Send future / SendWrapper 的关系

这篇落盘实现,从头到尾活在前几篇立下的 `Send` 纪律里。核心矛盾:所有 `JsValue`（`File`、
`FileSystemWritableFileStream`、`Promise`）都是 `!Send`,而端口 trait `FileAccess: Send + Sync`
（`crates/host/src/ports.rs:133`）——core 是按 `Send` future 写的(native 多线程需要),浏览器实现
必须把 `!Send` 兜成 `Send`。手法是 `send_wrapper::SendWrapper`(单线程 wasm 永不触发其跨线程 panic):

- **状态表整体裹**:`SendWrapper<RefCell<HashMap<..>>>` 让含 `!Send` 句柄的映射表满足 `Send + Sync`。
- **跨 await 只让 `SendWrapper` 过**:纪律是「裸 `RefCell` borrow / 裸 `!Send` 句柄绝不跨 await」。
  两种合法裹法——短路径在 scope 内取到 `Promise` 即丢,只让 `SendWrapper<JsFuture>` 跨 await
  (见上面 `write_sink_chunk`);多步 helper(如 `open_writable`:建目录链→取句柄→createWritable)
  则整段 future 裹 `SendWrapper`,内部 `!Send` 句柄随包一起兜。

而上一节删掉的 SyncAccessHandle 双路径,恰恰是**破坏这条纪律的**:二选一枚举逼着 `await` 落进
`match` 一个 `!Send` scrutinee 的臂里,直接撞 E0277。换句话说,那个 Send 边界坑不是偶然,是
「两种 `!Send` 写法并存」在 wasm 单线程 `Send` 纪律下的必然代价。**简化掉它,不只是少一套代码,
是让整个端口回到「一种 `!Send` 值、一种裹法」的干净状态。** 这和
[02 篇](02-n0-future-tokio-shim.md) 的 `Instant`、[05 篇](05-what-compiles-isnt-what-runs.md) 门 3
的 lost-wakeup 是同一族问题:**`!Send`/单线程不是编译错,是编译期看不见、要靠纪律和实测守住的第三维。**

## 小结

- **浏览器收大文件唯一干净出路 = OPFS + `createWritable` 流式 positioned write + `close` 提交**;
  内存 Blob 会 OOM,IndexedDB 没有 positioned write 语义。
- **读到多少写多少,内存占用是常量**;大文件不 OOM 靠的是「没有 buffer」,不是更大的 buffer。
- **SyncAccessHandle 被实测否决**:六组基准 `~31±3 MB/s`,瓶颈在 relay+noise 不在落盘;它省的一次
  Promise 调度是噪声级,却要养独占锁、不一致取消语义、短写校验和一个 Send 边界坑——简化成单路径。
- **主线程落盘不卡 UI**:写永远追着网络跑,单次 Promise 往返远快于一块的到达间隔。
- **全程在 `SendWrapper`/`!Send` 纪律内**;删双路径,本质是让端口回到「一种 `!Send` 值、一种裹法」。

至此,「浏览器跑与桌面字面同一份 `swarmdrop-transfer`」这条线在落盘侧闭环了(OPFS 逐字节一致)。
想看这块落盘设施背后的 Web 平台知识——OPFS 与 IndexedDB 的本质区别、secure context gating、
webrtc-websys 为何只能跑主线程——去 [browser-platform/00 — OPFS：浏览器里的私有文件系统](../browser-platform/00-opfs.md);
想看让 core 零 cfg 复用的那条端口边界怎么设计的,去
[transfer-architecture/02 — 依赖倒置:端口 trait](../transfer-architecture/02-dependency-inversion-ports.md)。
