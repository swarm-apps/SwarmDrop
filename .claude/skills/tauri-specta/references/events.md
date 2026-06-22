# 类型化事件

## 为什么需要

原生 Tauri 的 `emit` / `listen` 是字符串地狱：

```rust
app.emit("user-created", &user);            // 后端：随便起名
```

```ts
listen<???>("user-created", e => { ... });  // 前端：payload 类型靠猜
```

tauri-specta 把事件 wrapper 当成"既是 `specta::Type` 又是 `Event` trait 实现者"来收集，于是前后端都拿到强类型 payload + 自动同步的事件名。

## 一个最小例子

```rust
use serde::{Serialize, Deserialize};
use specta::Type;
use tauri_specta::Event;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct UserCreated(pub User);
```

注册到 builder：

```rust
SpectaBuilder::<Wry>::new()
    .events(collect_events![UserCreated]);
```

setup hook 里挂载：

```rust
.setup(move |app| {
    specta.mount_events(app);   // ← 没这行 emit 会 panic
    Ok(())
})
```

后端发：

```rust
use tauri_specta::Event as _;

UserCreated(user).emit(&app)?;
```

前端听：

```ts
import { events } from "./bindings";

events.userCreated.listen(e => {
    e.payload;  // ✅ User，自动补全
});
```

## `#[derive(tauri_specta::Event)]` 的属性

来自 tauri-specta macros 源码：

```rust
#[derive(Debug, Clone, Serialize, Type, Event)]
#[tauri_specta(event_name = "myCustomEvent")]  // 可选：覆盖默认事件名
pub struct UserCreated(pub User);
```

| 属性 | 说明 |
|------|------|
| 无（默认） | 用 struct 名转 kebab-case：`UserCreated` → `"user-created"` |
| `event_name = "..."` | 自定义事件名（**注意**：影响前端 import 名，但不影响 wire 字符串——见下） |

**默认就够用**，PascalCase struct → kebab-case wire name 是 Tauri 生态约定，前端属性名会被 specta-typescript 转成 camelCase。

```rust
pub struct PairedDeviceAdded(...);
```

```ts
// 生成
export const events = {
    pairedDeviceAdded: makeEvent<PairedDeviceAdded>("paired-device-added"),
    //  ^^^^^^^^^^^^^^^  camelCase（前端 import 名）
    //                                    ^^^^^^^^^^^^^^^^^^^^^^^^ kebab-case (wire name)
};
```

## newtype + `#[serde(transparent)]` 模式（推荐）

直接拿业务类型 derive `Event` 也行，但更好的做法是**用 newtype 包一层**：

```rust
// 业务 payload —— 在 core / 业务层定义，可能不属于桌面壳
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PairedDeviceInfo {
    pub peer_id: String,
    pub name: String,
}

// 事件 wrapper —— 在桌面壳定义
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(transparent)]                          // ← 关键
pub struct PairedDeviceAdded(pub PairedDeviceInfo);
```

`#[serde(transparent)]` 让 wire payload 与原 payload **形状完全一致**——前端 listener 拿到的就是 `PairedDeviceInfo`，没有额外嵌套。

**这种模式的好处**：

1. 业务类型（`PairedDeviceInfo`）可以是 core crate 的纯数据类型，不依赖 `tauri_specta::Event`
2. 事件 wrapper 集中在 `events.rs` 一个文件，方便审计
3. 多个事件可以包同一个 payload（如 `UserCreated(User)` / `UserUpdated(User)`）
4. wire 协议保持稳定——从手写字符串迁移过来时前端零改动

## `Event` trait 完整方法

`#[derive(Event)]` 自动 impl 这个 trait（来自 tauri-specta 源码）：

```rust
pub trait Event: Type + 'static {
    const NAME: &'static str;

    /// 监听本 Manager 上的事件
    fn listen<F, R, H>(handle: &H, handler: F) -> EventId
    where F: Fn(TypedEvent<Self>) + Send + 'static, Self: DeserializeOwned;

    /// 监听任意 target 的事件
    fn listen_any<F, R, H>(handle: &H, handler: F) -> EventId;

    /// 监听一次后自动注销
    fn once<F, R, H>(handle: &H, handler: F) -> EventId;

    /// 任意 target 监听一次
    fn once_any<F, R, H>(handle: &H, handler: F) -> EventId;

    /// 广播到所有 targets
    fn emit<R, H>(&self, handle: &H) -> tauri::Result<()>
    where Self: Serialize + Clone;

    /// 广播到指定 target
    fn emit_to<R, H, I: Into<EventTarget>>(&self, handle: &H, target: I) -> tauri::Result<()>;

    /// 按 filter 函数过滤 target 广播
    fn emit_filter<F, R, H>(&self, handle: &H, filter: F) -> tauri::Result<()>
    where F: Fn(&EventTarget) -> bool;
}
```

`TypedEvent<T>`：

```rust
pub struct TypedEvent<T: Event> {
    pub id: EventId,
    pub payload: T,
}
```

### 前端方法

```ts
import { events } from "./bindings";

// 监听（全局）
const unlisten = await events.userCreated.listen(e => {
    e.payload;  // User
});
await unlisten();

// 监听单个窗口
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
const w = getCurrentWebviewWindow();
await events.userCreated(w).listen(e => { ... });

// emit 到后端 + 所有窗口
await events.userCreated.emit(newUser);

// emit 到指定窗口
await events.userCreated(w).emit(newUser);
```

## 后端 emit 的典型场景

```rust
use tauri_specta::Event as _;

// 业务方法触发的事件
async fn create_user(name: String, app: AppHandle) -> Result<User, AppError> {
    let user = save_user(name).await?;
    UserCreated(user.clone()).emit(&app)?;
    Ok(user)
}

// 后台任务的事件
tokio::spawn(async move {
    loop {
        let status = check_network().await;
        NetworkStatusChanged(status).emit(&app).ok();
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
});
```

## 与原生 `emit` 的互操作

`mount_events` 注册的事件名与 Tauri 原生 emit 的字符串**完全等价**。新旧两套可以并存：

```rust
// 旧代码：仍然能 emit
app.emit("user-created", &user)?;

// 新代码：等价
UserCreated(user).emit(&app)?;
```

前端的 listener 不变。这让从原生 emit 迁移成本极低——逐个事件改，不影响其他。

## 跨 crate 事件分发的模式

如果 core crate 想触发事件，但又不想直接依赖 `tauri_specta::Event`，可以用 trait + adapter：

```rust
// core/src/host.rs
#[async_trait]
pub trait EventBus: Send + Sync + 'static {
    async fn publish(&self, event: CoreEvent) -> Result<(), Error>;
}

pub enum CoreEvent {
    UserCreated { user: User },
    NetworkChanged { status: NetworkStatus },
    // ...
}
```

```rust
// src-tauri/src/host/event_bus.rs
pub struct TauriEventBus {
    pub app: AppHandle,
}

#[async_trait]
impl EventBus for TauriEventBus {
    async fn publish(&self, event: CoreEvent) -> Result<(), Error> {
        match event {
            CoreEvent::UserCreated { user } => {
                UserCreated(user).emit(&self.app).map_err(...)?;
            }
            CoreEvent::NetworkChanged { status } => {
                NetworkStatusChanged(status).emit(&self.app).map_err(...)?;
            }
        }
        Ok(())
    }
}
```

→ core 只产 `CoreEvent` plain enum，桌面壳的 `TauriEventBus` 负责翻译成 tauri-specta typed event。同一个 core 可以被多个 host（Tauri 桌面、CLI、Web server）复用，每个 host 自己实现 `EventBus`。

## 常见错误

| 错误 | 原因 | 修法 |
|------|------|------|
| `EventRegistry not found in Tauri state` | 没调 `specta.mount_events(app)` | 在 setup hook 加上 |
| `Event xxx not found in registry!` | 注册了 type 但没 `collect_events![]` | 加进 `collect_events!` |
| `the trait Event is not implemented for ...` | 缺 `#[derive(Event)]` 或 `features` 没开 `derive` | 加 derive 或开 feature |
| 编译错：`tauri_specta::Event` 找不到 macro | `use tauri_specta::Event;` 把 trait import 进来，遮蔽了同名 derive macro | derive 写完整路径：`#[derive(tauri_specta::Event)]` |

特别说一下最后一个——trait 与 derive macro 同名是 tauri-specta 的特意设计（trait 用来调 `.emit()`，macro 用来 derive），但 Rust resolver 会优先把 `Event` 解析为 trait。两种解决方法：

```rust
// 方法 1：derive 写完整路径
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct Foo;

// 方法 2：用 `use tauri_specta::Event as _;` 只 import trait 的方法
use tauri_specta::Event as _;
Foo.emit(&app)?;
```

## 相关
- [setup.md](setup.md) — `mount_events` 的位置
- [commands.md](commands.md) — `Channel` vs Event 的取舍
- [types.md](types.md) — payload 类型的 specta::Type 配置
- [pitfalls.md](pitfalls.md) — `kebab` rename 反作用、name collision 等坑
