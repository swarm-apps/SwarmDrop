# Rust Stream → JS ReadableStream

> **讲什么**：把 Rust 侧的事件流（`futures::Stream`）交给浏览器 JS 消费的标准桥——
> `serde-wasm-bindgen` 序列化 + `wasm-streams` 的 `ReadableStream`，JS 侧用 `getReader()`
> 单点消费再 fan-out。讲清楚三个坑：ReadableStream 同一时刻只能一个 reader、serde tag /
> camelCase 要和 JS 对齐、以及 Rust 侧改字段 TS 侧不会报错（没有自动生成的类型）。
>
> **为什么重要**：Rust 传输内核在浏览器里跑，进度/offer/完成这些事件必须实时推给 UI。
> Rust 的 async Stream 和 JS 的事件消费之间没有天然桥，`ReadableStream` 是 Web 平台里
> 唯一现成的「异步可读序列」原语。用错它的 reader 模型，会在调试时看到「事件全丢」的假象。

## 问题：Rust 的事件怎么流到 JS

本项目的传输内核（`swarmdrop-transfer`）在运行时会不断产出事件：收到 offer、进度更新、
完成、失败、暂停……这些在桌面端经 Tauri Channel/emit 推给前端。到了浏览器，没有 Tauri，
Rust（wasm）和 JS 之间只有 wasm-bindgen 这一层 ABI。

事件是**一个随时间产出的异步序列**。Web 平台里表达「异步可读序列」的标准原语是
`ReadableStream`（Streams API）。桥的两端是：

- Rust 侧：`futures::Stream<Item = Event>`（无界 channel 的 receiver）。
- JS 侧：`ReadableStream`，`getReader()` 出一个 reader，`while (reader.read())` 消费。

## 桥的三段：channel → 序列化 → ReadableStream

本项目的实现分三段，落在两个文件里。

**第一段：事件进无界 channel（`crates/web/src/events.rs`）。** 传输内核通过
`TransferEventSink` 端口发事件，Web 实现把它塞进 `futures::channel::mpsc::unbounded`：

```rust
// crates/web/src/events.rs
pub struct WebEventSink { tx: mpsc::UnboundedSender<TransferEvent> }

#[async_trait]
impl TransferEventSink for WebEventSink {
    async fn emit(&self, event: TransferEvent) -> AppResult<()> {
        let _ = self.tx.unbounded_send(event);   // 消费方 drop 时静默丢弃，不影响传输
        Ok(())
    }
}
```

**第二段：receiver → ReadableStream（`crates/web/src/node.rs` 的 `events()`）。**
`wasm_streams::ReadableStream::from_stream` 把一个 `Stream<Item = Result<JsValue, JsValue>>`
包成 JS 的 `ReadableStream`，每个 item 现场序列化：

```rust
// crates/web/src/node.rs
pub fn events(&self) -> Result<web_sys::ReadableStream, JsValue> {
    let rx = self.events_rx.borrow_mut().take()          // ← take：只能取一次，见下
        .ok_or_else(|| WebError::invalid_input("events() 只能取一次"))?;
    let stream = rx.map(|ev| Ok::<JsValue, JsValue>(crate::events::serialize_event(ev)));
    Ok(wasm_streams::ReadableStream::from_stream(stream).into_raw())
}
```

**第三段：JS getReader() 单点消费（`crates/web/static/index.html`）。**

```javascript
const reader = node.events().getReader();
while (true) {
  const { value: ev, done } = await reader.read();
  if (done) break;
  if (!ev) continue;                 // 序列化失败会产出 null，跳过
  switch (ev.type) {
    case "transferOfferReceived": renderOffer(ev.offer); break;
    case "transferProgress": /* 更新进度条 */ break;
    // ...
  }
}
```

```mermaid
graph LR
    K["传输内核"] -->|"emit(TransferEvent)"| CH["mpsc::unbounded"]
    CH -->|".map(serialize_event)"| RS["wasm_streams::ReadableStream::from_stream"]
    RS -->|"into_raw()"| JS["JS ReadableStream"]
    JS -->|"getReader().read()"| UI["单点消费 → fan-out 到各 UI"]
```

这套「每个协议/连接自产一路 Stream，转成 ReadableStream 交给 JS」的范式，直接对标 iroh
官方 `browser-chat` 的 `into_js_readable_stream` 6 行 helper
（见 [iroh 06-wasm-browser.md](../../../.claude/skills/iroh/references/06-wasm-browser.md)）。

## 坑一：ReadableStream 同一时刻只能一个 reader

这是 Web 平台的硬规则，也是本项目调试时踩过的**观测假象**来源：**一个 `ReadableStream`
在任一时刻只能有一个 active reader**。调用 `getReader()` 会把流「锁定」（locked），
在已锁定的流上**再次** `getReader()` 直接抛 `TypeError`。

调试时的坑长这样：以为「多调一次 `events()` 或多 `getReader()` 一次能多订阅一路」，
结果第二次 `getReader()` 报错，或者以为「事件没到」——其实是流被第一个 reader 独占了。

本项目用两个设计避开它：

1. **`events()` 用 `take()`，只能取一次**（见上）。第二次调 `events()` 直接返回
   `WebError::invalid_input("events() 只能取一次")`，把「单点消费」这条约束显式化，
   而不是留给 JS 侧撞 locked。
2. **JS 侧单点消费再 fan-out**：`index.html` 全程只 `getReader()` 一次，在那个循环里
   `switch (ev.type)` 分发到不同 UI 区域。要多组件订阅，正确姿势是「一个地方读、
   自己往下分发」，不是「每个组件各 getReader 一次」。

> iroh `browser-chat` 的 `iroh.ts` 也是这个模式——全 frontend 只 `getReader()` 一次，
> 在一个 class 里 fan-out 给 `subscribers` 数组。别被「clone 一个 receiver 句柄」骗了：
> clone 的是句柄，底层还是同一个流，还是只能一个 reader。

## 坑二：serde tag / camelCase 要和 JS case 对齐

JS 侧靠 `ev.type` 分派（`switch (ev.type)`）、靠 `ev.event.sessionId` 取字段。这些字符串
**完全由 Rust 侧的 serde 属性决定**，对不齐就静默匹配不上。

本项目的镜像枚举把 tag 和大小写都标死（`crates/web/src/events.rs`）：

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]   // ← variant 名 → ev.type
enum WebTransferEvent {
    TransferOfferReceived { offer: TransferOfferEvent },   // → "transferOfferReceived"
    TransferProgress { event: TransferProgressEvent },     // → "transferProgress"
    // ...
}
```

两个必须注意的点：

- **`tag = "type"`** 决定 JS 读 `ev.type`；内部 tag 让每个事件序列化成
  `{ type: "transferProgress", event: {...} }` 这种带判别字段的对象，JS `switch` 才有的分。
- **`rename_all = "camelCase"` 只改 variant 名，不自动改嵌套结构体的字段名**。字段的
  camelCase（如 `sessionId` / `transferredBytes`）要在**每个 payload 结构体**上各自标
  `#[serde(rename_all = "camelCase")]`。iroh 官方 `browser-echo` 就在这里漏标过——
  外层标了 camelCase，内层字段到 JS 还是 snake_case，因为只做 `JSON.stringify` 展示
  才没暴露。本项目 `index.html` 直接读 `ev.event.sessionId`，漏标就会拿到 `undefined`。

为什么用「镜像枚举」而不直接给 `TransferEvent` derive `Serialize`？因为 transfer 域
crate 不想为 Web 的序列化格式买单——Web 侧定义一个 1:1 的 `WebTransferEvent` 镜像，
`From<TransferEvent>` 转换，序列化格式的约定留在 Web 壳里（和桌面把 `TransferEvent`
映射进 `CoreEvent` 的适配范式一致）。

## 坑三：没有从 Rust 自动生成 TS 类型

wasm-bindgen 只为**导出的 struct/fn** 生成 `.d.ts`；经 `serde_wasm_bindgen` 转出去的
`JsValue`，在 TS 侧就是 `any`。生成的 `.d.ts` 里 `events()` 的返回类型是**裸
`ReadableStream`，没有泛型参数**——JS 拿到的每个事件都是 untyped。

后果：**Rust 侧改了字段名，TS 侧静默不报**。`ev.event.sessionId` 改成
`ev.event.session_id` 后，JS 依然编译通过，只是运行时全 `undefined`。iroh `browser-chat`
的处理是「在 `iroh.ts` 里手写一遍 `ChatEvent` 类型，消费处 `as` 断言接上」——手动维护，
容易和 Rust 漂移。

本项目当前是 vanilla JS demo（`index.html`），没上 TS 类型，靠约定对齐。真要做认真的
Web 前端，需要**自己补一个类型生成步骤**（specta 对 wasm 边界导出 TS，或 ts-rs），
否则 Rust↔TS 的事件契约没有编译期保护。

## 健壮性：坏事件不能掀翻整个循环

一个实战细节：`reader.read()` 的循环里，**单个坏事件不能让整个 reader 循环崩掉**
（否则后续事件全静默）。本项目两侧都做了容错：

- Rust 侧 `serialize_event` 序列化失败**不静默吞成 null 就完**——先 `warn` 打出错因
  （否则「事件没到 JS」极难定位），再返回 `JsValue::NULL`（reader 侧对 null 有容错）。
- JS 侧每个事件处理套 `try/catch`，坏事件跳过并记日志，`reader.read()` 抛错才 break。

## 小结

- Rust `Stream` → JS 的标准桥：`mpsc` channel →（`serialize_event`）→
  `wasm_streams::ReadableStream::from_stream` → JS `getReader()`。
- **ReadableStream 同一时刻只能一个 reader**（第二次 getReader 抛 TypeError）。本项目用
  `take()` 强制「events() 只取一次」+ JS 单点消费 fan-out 避坑。
- serde 的 `tag` / `rename_all` 决定 JS 读的 `ev.type` / 字段名；`rename_all` **只改
  variant 名，嵌套字段要各自标**，否则 JS 拿 undefined。
- 没有从 Rust 自动生成的 TS 类型——`events()` 返回裸 `ReadableStream`，Rust 改字段
  TS 静默不报；认真做要补 specta/ts-rs。

**下一篇** [05-wasm-bindgen-boundary.md](05-wasm-bindgen-boundary.md) 收尾整个 wasm↔JS
边界：newtype、SendWrapper、可序列化 error 这三类约束。
