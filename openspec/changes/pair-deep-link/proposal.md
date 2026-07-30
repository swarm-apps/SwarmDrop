## Why

`invite-url-canonical` 让邀请变成一条可点、可扫的链接，落地页给出「在 App 中打开」。
但那个按钮现在**点不通** —— 桌面没有注册任何 URL scheme，移动端 `app.json` 虽然有
`"scheme": "swarmdrop"`（Expo 默认注册），却**没有任何 Linking 监听**（`mobile/src` 下只有
出站的 `openURL` / `openSettings`）。链接能打开落地页，但走不进应用。

同时剪贴板感知这条离线兜底路径也有两个缺口：

- **只在配对页生效**。`use-clipboard-invite.ts` 的 `enabled` 只有 `/pairing/input` 传 true，
  用户从别处进 App 时不会被感知；而且去重状态 `seen` 是组件内 state，换个路由就忘了，
  同一条邀请会反复亮条。
- **会对自己复制的邀请亮条**。用户复制自己生成的邀请准备发给别人，回到 App 反被问「要不要
  配对」—— 这是噪音，而且是最常见的一次操作。

`pair-invite-ui/design.md` D4 当年把深链留成独立 change 时，已经点明了要做的两件事：
`tauri-plugin-deep-link` + **macOS 上深链与 share-target 都钩 `RunEvent::Opened` 的分流**。
这个 change 就是它。

## What Changes

- **桌面注册 URL scheme**：引入 `tauri-plugin-deep-link`，但**只用它的注册能力**
  （Windows 注册表 / Linux `.desktop` / macOS plist），macOS 的事件消费仍走自己那份
  `RunEvent::Opened` handler —— 见 design D1。
- **`external_open.rs` 泛化成外部入口分发器**：`file://` → share-target、`swarmdrop://` →
  pairing。冷启动缓冲（全局 `OnceLock` + `take_pending`）、200ms 去抖、`catch_unwind`
  三样现成机制**复用而非复制** —— 深链遇到的是同一批问题（尤其冷启动时事件早于前端 mount）。
- **Android 深链**：`app.json` 加 intent-filter，RN 侧加 `Linking` 监听（冷启动
  `getInitialURL` + 运行时 `addEventListener`），payload 走统一解析入口。
  **iOS 本期不做**（用户确认）—— 缺口在 design D3 明说。
- **剪贴板检测改造**：
  - 范围从配对页提到 `_app` 布局（全局），去重状态挪进 store（跨路由保持）
  - 呈现保持**非模态**（顶部条 / toast），点击才进模态确认卡 —— 用户已认可
  - **自我过滤**：decode 后若 `inviter_id == 本机 NodeId`，静默忽略。判据是签名覆盖范围内的
    结构性字段，零额外状态 —— 见 design D4
  - decode 提前到亮条之前，于是提示条能直接写出对端设备名（「张三的 MacBook 想和你配对」），
    而不是现在那句「检测到配对邀请」

**非目标**：iOS 深链与 Universal Link；Android App Links 的域名验证（`assetlinks.json` 依赖
备案与可达性，见 design D6，本期只做 custom scheme）；剪贴板读的原生化
（→ `fix-clipboard-native-read`，独立先合）；载体形态（→ `invite-url-canonical`，本 change 依赖它）。

## Capabilities

### New Capabilities

- `pair-deep-link`: 邀请链接可从浏览器移交进应用 —— 桌面三平台与 Android 注册 `swarmdrop://`
  并把 payload 送进配对流程，冷启动与已运行两种时序都不丢事件；外部入口（打开方式 / 深链）
  在宿主层统一分发。

### Modified Capabilities

- `pair-invite-ui`: 剪贴板感知从配对页局部提升为全局，去重跨路由保持，本机自己生成的邀请
  不再触发提示，提示条携带对端身份信息。

## Impact

- **`src-tauri`**：`Cargo.toml` + `package.json` 加 deep-link plugin；`setup.rs` 注册；
  `external_open.rs` 泛化为分发器（模块内新增 URL 分流，对外多一个 pairing 事件）；
  `events.rs` 加深链事件类型；`capabilities/default.json` 按 plugin 要求补权限；
  `tauri.conf.json` 配 scheme。
- **`src/`**：剪贴板 hook 提到 `_app.tsx`；去重状态进 `pairing-store`；提示条组件改造
  （带设备名）；深链事件的前端处理器（与 `ExternalFileOpen` 的 `take_pending` 同构）。
- **`mobile/`**：`app.json` 的 Android intent-filter；`Linking` 监听接进根布局；
  深链 payload → 配对确认流；剪贴板自我过滤（Android 可读，iOS 见 D3）。
- **`docs/`**：落地页「在 App 中打开」按钮的 scheme 拼装（payload 从 `location.hash` 显式取，
  不依赖系统携带 —— `invite-url-canonical` design D4）。
- **回归**：`cargo test --workspace`；桌面三平台冷启动/热启动各点一次链接；
  Android 冷启动/热启动各点一次；share-target（打开方式）在同一批改动后仍正常 ——
  **这是本 change 最需要防的回归**。

**风险**：

1. **macOS 事件抢占**（design D1 / D2）：plugin 与自建 handler 都钩 `RunEvent::Opened`，
   谁先拿到、会不会互相吃掉事件必须实测。`pair-invite-ui/design.md` D4 一年前就标了这条要 PoC。
2. **动到已经跑通的崩溃防护路径**。`external_open.rs` 的 `catch_unwind` 是踩过真实
   abort 才加的（ObjC `extern "C"` 边界 panic 不可 unwind），泛化时不能把它绕开。
3. **share-target 回归**。文件打开与深链共用一条分发路径后，`file://` 分支必须保持行为不变。
