## Why

### 1. 设备名是唯一没有端口的宿主能力

`crates/host/src/ports.rs` 有 5 个端口 trait —— `KeychainProvider`(:43)、`AppPaths`(:75)、
`FileAccess`(:150)、`Notifier`(:211)、`UpdateInstaller`(:230)。设备名不在其中，于是三端各写一份、
零共享代码：

| 端 | 存在哪 | 谁读得到 |
|---|---|---|
| 桌面 | `app_data_dir/device_config.json`（`src-tauri/src/host/device_config.rs:24`），两个自由函数 `load_device_name`(:65) / `save_device_name`(:70) | 只有 `src-tauri` |
| 移动 | JS 侧 AsyncStorage（`mobile/src/stores/preferences-store.ts:88`/`:99`/`:281`） | 只有 JS。Rust 侧在 `network.rs:194` 收 `start_node` 的入参、`:216` 拿去 `OsInfo::native(device_name)`，**构造完即丢** |
| Web | **不存在**。`crates/web/src/node.rs:702` 的 `name` 硬编码 `None` | —— |

桌面那份文件头 :7-9 把「故意不暴露 trait」写成了设计决策：

> 设计上故意不暴露 trait —— 调用方只通过 `load_device_name` / `save_device_name` 两个函数交互

当时只有一个宿主，这是对的。三端之后它的代价是：Web 端要设备名就得从零再写一份，而没人写，
于是 GitHub issue #103 —— Web 设备名从 UA 派生（`Chrome` / `Edge` / `Firefox` / `Safari` /
`Browser` 五选一）且用户改不了。对端设备列表里会出现一排无法区分的「Chrome」，而那一行字
正是对端决定「这台要不要收」的主要线索。

### 2. 用户设的名字进不了配对请求，也进不了邀请串（存量 bug，三端都中）

生产代码里还有三处 `OsInfo::default()`，把已经存在的设备名整个绕开：

- **`crates/core/src/pairing/manager.rs:287`** —— `request_pairing` 构造 `PairingRequest` 时
  `os_info: OsInfo::default()`。这条 `os_info` 会经 `CoreEvent::PairingRequestReceived` 送到
  **对端的配对确认弹窗**（`src/components/pairing/connection-request-dialog.tsx:74` 的
  `deviceDisplayName(incomingRequest.osInfo)`）与系统通知（`manager.rs` handle_inbound 取
  `req.os_info.hostname`）。`OsInfo::default()` 的 `name` 恒为 `None`，所以对方在**决定要不要
  接受配对的那一刻**看到的永远是机器 hostname，不是你起的名字；浏览器端更糟 —— wasm 下
  `std::env::consts::OS` 恒 `"unknown"`、没有 `COMPUTERNAME`/`HOSTNAME`，于是显示成
  「Device · unknown」，而 `web_os_info()` 辛苦算出来的「Chrome · web」压根没被用上。
- **`src-tauri/src/commands/pairing.rs:83`** —— 桌面生成邀请用 `OsInfo::default()`，
  邀请串的 `display_name` 因此是 hostname 而非用户设的名字。
- **`mobile/packages/swarmdrop-core/rust/mobile-core/src/pairing.rs:100`** —— 同上，且移动端
  `OsInfo::default()` 拿不到任何环境变量、hostname 回落 `"Device"`，**移动端发出的邀请
  display_name 恒为「Device」**。

根因是「本机 OsInfo 没有单一事实源」：`start_node` 收到了一份完整的 `os_info`
（`crates/core/src/runtime.rs:83`），但只用它算了一次 `agent_version`（:105）就丢掉，
`PairingManager` 拿不到，于是每个需要它的地方就地 `OsInfo::default()` 造一个假的。

**这不是 Web 独有的缺口，桌面上也是坏的**，所以本 change 里它是 bug fix，不是新功能。

### 3. 顺带：一个已在文档里承诺、实际失效的隐私开关

`src-tauri/src/host/paths.rs`（2078 字节）实现了 `SWARMDROP_DATA_DIR` fixture 覆盖，
但 `src-tauri/src/host.rs` 的模块声明列表里**没有 `pub mod paths;`** —— 整个文件从未参与编译。
三个「调用方」各自直读 Tauri API：`device_config.rs:25`、`file_keychain.rs:50`、`database.rs:24`。

它不只是死代码。`e2e/desktop/demo-asset-plan.md:69` 仍在宣传它，而录制脚本按那份文档
`SWARMDROP_DATA_DIR=$FIXTURE` 起 app —— seeder（`crates/core/examples/seed_demo_profile.rs:52`）
确实往 fixture 目录写假身份，app 却读平台默认目录。结果是**演示录制读的是真实 profile**，
真实设备名 / peer ID 会入镜，正好违反同一份文档 §6 的隐私要求。

## What Changes

- **新增 `DeviceConfig` 端口**（`crates/host/src/ports.rs`）：`load_device_name` /
  `save_device_name`。推翻 `device_config.rs:7-9` 的「故意不暴露 trait」——那个决策的前提
  （只有一个宿主）已经不成立。load 无错误返回（配置读坏不该挡住节点启动），save 返回
  `AppResult<()>`（用户按了保存，失败必须让他知道）—— 取舍见 design D2。

- **新增 `DeviceName` newtype**（`crates/host/src/device.rs`）：唯一构造入口
  `DeviceName::parse` 做 trim / 空串→`None` / 截断 40 字符 / **剥掉 `;` 与控制字符**。
  端口签名吃 `Option<DeviceName>`，于是「未归一化的名字存不进去」是类型保证而不是纪律。
  剥 `;` 不是洁癖：`OsInfo::to_agent_version()`（`crates/host/src/device.rs`）用 `"; "` 拼串、
  `from_agent_version()` 按 `"; "` 切，把设备名改成 `我的电脑; caps=lan-helper` 就能让对端
  `event_loop.rs:123` 认下这个 capability，进而 `add_infrastructure_peer(kad_server+relay)`
  —— 这条今天在桌面/移动上已经可触发，见 design D3。

- **core 的组合根成为本机 OsInfo 的唯一装配点**：`start_node` 增收
  `Arc<dyn DeviceConfig>`，在内部 `os_info.name = device_config.load_device_name()`；
  `OsInfo::native()` **去掉 `name` 参数**，让宿主结构性地无法再自己注名字。
  `PairingManager` 增持 `os_info` 字段，由组合根注入，消掉上面三处 `OsInfo::default()`；
  `encode_invite` 随之**去掉 `display: &OsInfo` 参数**（三端各传一份 display 正是分叉的来源）。

- **三端补齐实现**：
  - 桌面 `DesktopDeviceConfig` —— 两个自由函数包成端口实现，存储格式不变。
  - 移动 `MobileDeviceConfig` —— 设备名从 JS AsyncStorage 挪进 Rust 侧，写
    `data_dir/device_config.json`（`data_dir` 已在 `MobileCore::new` 里，SQLite 就用它）。
    新增 `get_device_name` / `set_device_name` uniffi 导出；`start_node` 的 `device_name`
    入参删除。取舍（Rust 落盘 vs uniffi callback 回 AsyncStorage）见 design D4。
  - Web `IdbDeviceConfig` —— IndexedDB `kv` store 新增一个键（与身份同域）；新增
    `get_device_name` / `set_device_name` / `default_device_name` 三个**模块级** wasm 导出
    （不挂 `WebNode`，节点起不来时设置页仍要能改名）。`web_os_info()` 的 UA 派生结果
    **保持不动**：它写在 `hostname` 字段上，而 `OsInfo::display_name()` 本来就是
    `name || hostname`，所以「UA 结果降级为默认值」只需要让 `name` 有来源，不需要改 UA 逻辑；
    第三个导出的存在是为了让设置页的 placeholder 与对外表示**同源**，而不是在 TS 里再写一份
    UA 解析（design D5）。

- **Web 设置页加设备名编辑入口**（`docs/app/app/_components/node-panel.tsx`，issue #103 指定的
  「本机节点」块）。**不做首次进入的强制命名引导** —— Web 的产品前提是「点开链接就能用」
  （design D6）。

- **桌面 / 移动的「改名后重启节点」语义对齐**：`src/lib/device-name.ts:29-37` 与
  `mobile/src/lib/device-name.ts:38-49` 现在一个 `console.warn` 吞掉重启失败（UI 却弹
  「设备名称已更新」）、一个 `throw`。统一成「保存与重启是两件事，分别反馈」（design D7）。

- **`paths.rs` 接线而非删除**，并修 `demo-asset-plan.md` 的描述；`AppPaths` 端口删除
  （零生产实现、零生产消费，唯一 impl 是测试替身 `MemoryHost`）—— 两者的判据见 design D8 / D9。

**非目标**：

- **改名后让已连接的对端立刻看到新名字**（identify `agent_version` 运行时推送）→ `C6
  identify-agent-version-runtime-update`。本 change 的过渡语义是显式的：桌面/移动改名后
  shutdown + start，Web 提示刷新页面。这条限制写进 spec，并注明 C6 会消除它。
- **解除配对的原子化与 `PairedDeviceStore` 端口** → `C4
  atomic-unpair-and-paired-device-store`。本 change 只与它有**合并顺序**上的依赖
  （同改 `ports.rs` 与组合根签名），无语义依赖。
- **传输 / 收件箱端口补全** → `C2` / `C3`。
- 设备名的多语言默认值、按平台建议名（移动端 `suggestedDeviceName` 保持现状）。

## Capabilities

### New Capabilities

- `device-naming`: 本机设备名是一项可配置、跨重启持久、三端同构的能力 —— 有唯一的宿主端口、
  唯一的归一化入口、唯一的装配点，并被邀请串、配对请求与 identify `agent_version` 三条对外
  表示共同消费。

## Impact

- **`crates/host`**：`ports.rs` 加 `DeviceConfig`、删 `AppPaths` + `CoreAppPaths`；
  `device.rs` 加 `DeviceName` newtype 与 `normalize` 单测，`OsInfo::native()` 去参。
- **`crates/core`**：`runtime.rs` 的 `start_node` 增一个端口参数并在内部填 `os_info.name`；
  `network/manager.rs` 的 `NetManager::new` 透传 `os_info`；`pairing/manager.rs` 增
  `os_info` 字段、`encode_invite` 去 `display` 参、`request_pairing` 与入站通知改用它；
  `host.rs` 的 `MemoryHost` 去掉 `paths` 字段。
- **`src-tauri`**：`host/device_config.rs` 包成 `DesktopDeviceConfig`（内部 `struct DeviceConfig`
  与端口重名，改名 `DeviceConfigFile`）；`host.rs` 声明 `pub mod paths;` 并让三个调用方经它取目录；
  `commands/lifecycle.rs:55-60` 与 `commands/pairing.rs:83` 随签名变化调整；
  `commands/identity.rs` 的 `set_device_name` 改用 `DeviceName::parse`。
- **`mobile/`**：新增 `mobile-core/src/device_config.rs` + 两个 uniffi 导出；
  `network.rs` 的 `start_node` 去 `device_name` 参；JS 侧 `mobile-core-store.ts:200-201`、
  `lib/device-name.ts`、`preferences-store` 的 `deviceName` 降级为显示镜像 + 一次性迁移。
- **`crates/web`**：新增 `device_config.rs`（`IdbDeviceConfig` + 三个模块级 wasm 导出）；
  `node.rs` 删 `os_info` 字段（`generate_invite` 不再需要它）、`web_os_info()` 提到
  `pub(crate)` 供新导出复用。
- **`docs/`**：`node-panel.tsx` 加编辑入口、`_lib/store.ts` 加 `deviceName` 域、
  `_lib/node-runtime.ts` 把私有的 `loadModule` 开成 `getModule()`（今天前端只能拿到
  `WebNode` 实例，够不到模块级导出）；`pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web`。
- **`e2e/desktop/demo-asset-plan.md`**：`SWARMDROP_DATA_DIR` 一节改成描述接线后的真实行为。
- **回归**：`cargo test --workspace`、`./scripts/check-wasm.sh`（含 `--clippy`）、
  `pnpm test`、`docs` 下 `pnpm build`、`mobile` 下 `pnpm typecheck`。

**风险**：

1. **移动端设备名存储位置迁移**。存量用户的名字在 AsyncStorage 里，迁移漏做就等于「升级后
   设备名被清空、对端看到 Device」。一次性迁移写在 JS bootstrap（`getDeviceName()` 为 null
   且本地镜像非空 → 推一次），必须实机验证升级路径而非只验全新安装。
2. **`start_node` 与 `encode_invite` 的签名同时变**，三端 + `crates/core/tests/` 的调用点
   一起改。编译期能全抓到，但 `crates/web` 只有 wasm target 才编真身 —— 必须跑
   `./scripts/check-wasm.sh`，`cargo check --workspace` 抓不到 Web 端的漏改。
3. **`paths.rs` 接线会改变 debug build 下三处存储的落点**（当 `SWARMDROP_DATA_DIR` 已设时）。
   开发机上若恰好设了这个变量，identity / device_config / SQLite 会一起搬家，表现为
   「身份变了、配对全丢」。release 不受影响（`cfg(debug_assertions)`），但接线后要在
   未设变量的干净环境里确认落点与今天一致。
