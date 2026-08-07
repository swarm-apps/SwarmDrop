# device-config-port 任务分解

> 依赖 `C4 atomic-unpair-and-paired-device-store`（同改 `crates/host/src/ports.rs` 与
> `start_node` 签名，合并顺序依赖；无语义依赖，见 design 开头）。
>
> 顺序上 Phase 1 是硬门槛：`OsInfo::native()` 去参 + `encode_invite` 去参会让三端一起编不过，
> 中途停手的仓库是不可编译的。Phase 1–4 应在同一批改动里推完。

## 1. 端口与共享类型（`crates/host`）

- [x] 1.1 `crates/host/src/device.rs` 新增 `DeviceName` newtype：私有 `String` 字段，
      唯一构造入口 `DeviceName::parse(raw: &str) -> Option<Self>`，另有 `as_str()` /
      `into_string()`。归一化顺序：trim → 剥控制字符与 `;` → 截断 40 个 char → 空则 `None`
- [x] 1.2 `crates/host/src/device.rs` 加 `DeviceName` 单测：空串 / 纯空白 → `None`；
      41 个中文字截断到 40 且不 panic（按 char 不按 byte）；`"我的电脑; caps=lan-helper"`
      解析后不含 `;`；已归一化的串 parse 是幂等的
- [x] 1.3 `crates/host/src/device.rs` 加**回归锚点测试**：把一个含 `"; caps=lan-helper"`
      的原始串经 `DeviceName::parse` → `OsInfo` → `to_agent_version()` →
      `from_agent_version()` 走一圈，断言解析出的 `capabilities` 为空。
      注释写明这条红了意味着 agent_version 分隔符注入回来了（design D3）
- [x] 1.4 `crates/host/src/ports.rs` 新增 `DeviceConfig` trait：
      `async fn load_device_name(&self) -> Option<DeviceName>` +
      `async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()>`。
      doc 注释写清 load 为何不返回错误（启动路径上，读坏必须降级到 hostname，design D2）
- [x] 1.5 `crates/host/src/device.rs` 的 `OsInfo::native(name: Option<String>)` 改为
      `OsInfo::native()`（去参），doc 注释改写为「`name` 由 core 组合根从 `DeviceConfig`
      端口填充，宿主无法注入」
- [x] 1.6 删除 `crates/host/src/ports.rs:75` 的 `AppPaths` trait 与 `CoreAppPaths` struct
      （design D9）

## 2. core：本机 OsInfo 的唯一装配点

- [x] 2.1 `crates/core/src/runtime.rs` 的 `start_node` 增参 `device_config: Arc<dyn DeviceConfig>`，
      在算 `agent_version` **之前**执行
      `os_info.name = device_config.load_device_name().await.map(DeviceName::into_string)`
- [x] 2.2 `crates/core/src/runtime.rs:97-99` 的注释改写：说明 `os_info` 只承载平台探测部分，
      `name` 由本函数从端口填充，宿主传入的值不再存在（`OsInfo::native()` 已去参）
- [x] 2.3 `crates/core/src/runtime.rs` 把装配好的 `os_info` 传进 `NetManager::new`
      （现在只用于 `agent_version` 后即丢）
- [x] 2.4 `crates/core/src/network/manager.rs:54` 的 `NetManager::new` 增参 `os_info: OsInfo`，
      透传给 `PairingManager::new`（:83）
- [x] 2.5 `crates/core/src/pairing/manager.rs:128` 的 `PairingManager` 增字段
      `os_info: OsInfo`；`new`（:151-169）增参
- [x] 2.6 `crates/core/src/pairing/manager.rs:287` 的 `os_info: OsInfo::default()` 改为
      `self.os_info.clone()` —— **本 change 的核心 bug fix**
- [x] 2.7 `crates/core/src/pairing/manager.rs` 的 `handle_inbound` 里入站通知
      `Notification::PairingRequest { hostname: req.os_info.hostname.clone() }` 改用
      `req.os_info.display_name()`（否则弹窗显示用户名、通知显示 hostname，自相矛盾）
- [x] 2.8 `crates/core/src/pairing/manager.rs:193-198` 的 `encode_invite` 去掉
      `display: &OsInfo` 参数，改读 `self.os_info`
- [x] 2.9 `crates/core/src/host.rs` 删 `MemoryHost` 的 `paths` 字段与 `impl AppPaths`（:258），
      `MemoryHost::new(paths)` → `MemoryHost::new()`
- [x] 2.10 `crates/core/src/host.rs:554-562` 删 `memory_host_should_return_configured_app_paths`
      测试（它测的对象已不存在）
- [x] 2.11 删四处 `test_paths()` helper 定义（`crates/core/tests/e2e_transfer.rs:62`、
      `crates/core/tests/e2e_lan_helper.rs:38`、`crates/core/src/identity.rs:162`、
      `src-tauri/src/database.rs:93`）与随之无用的 `CoreAppPaths` 导入，并把
      `MemoryHost::new(test_paths())` 全文替换为 `MemoryHost::new()` ——
      **约 30 处**，`e2e_transfer.rs` 里就有 20+（核实记录 8）。机械改动，编译期兜底
- [x] 2.12 `crates/core` 加两条测试，**直接构造 `PairingManager` 并注入一个已知
      `OsInfo`**（不经 `start_node` —— core 的 e2e 全都手抄了它的 body，那条路径没有 harness，
      见 design D11 / 核实记录 6）：
      ① `request_pairing` 发出的 `PairingRequest.os_info.name` == 注入值；
      ② `encode_invite` 产出的串 decode 后 `display_name` == 注入值

## 3. 桌面实现（`src-tauri`）

- [x] 3.1 `src-tauri/src/host/device_config.rs`：私有 `struct DeviceConfig`（:19）改名
      `DeviceConfigFile`（与端口 trait 重名）
- [x] 3.2 同文件新增 `pub struct DesktopDeviceConfig { app: AppHandle }` 并
      `impl DeviceConfig`，内部复用现有 `read` / `write`；`load` 侧对读出的串再走一次
      `DeviceName::parse`（防手改坏的 json 注入 agent_version）
- [x] 3.3 同文件删掉裸露的 `load_device_name`(:65) / `save_device_name`(:70)，
      改写文件头 :7-9 那段「故意不暴露 trait」的注释，记下推翻理由（三端之后前提不成立）
- [x] 3.4 `src-tauri/src/host.rs` 的模块头注释补一条 `device_config` → `DeviceConfig` 端口
- [x] 3.5 `src-tauri/src/commands/lifecycle.rs:55-60`：删 `load_device_name` 调用与
      `OsInfo::native(device_name)` 的入参，改为 `OsInfo::native()` + 向 `start_node`
      传 `Arc::new(DesktopDeviceConfig::new(app.clone()))`
- [x] 3.6 `src-tauri/src/commands/identity.rs:46-48` 的 `get_device_name` 改经端口
- [x] 3.7 `src-tauri/src/commands/identity.rs:59-66` 的 `set_device_name` 改为
      `DeviceName::parse` + 端口 save，删掉就地的 trim/filter
- [x] 3.8 `src-tauri/src/commands/pairing.rs:83` 删 `let os_info = OsInfo::default();`
      与 `encode_invite` 的第三个实参 —— **桌面侧 bug fix**
- [x] 3.9 `cargo test -p swarmdrop_lib export_ts_bindings`（或按仓内既有方式）重新导出
      `src/lib/bindings.ts`；确认 `getDeviceName` / `setDeviceName` 签名未变（应无变化）

## 4. 移动实现（`mobile/`）

- [x] 4.1 新增 `mobile/packages/swarmdrop-core/rust/mobile-core/src/device_config.rs`：
      `MobileDeviceConfig { data_dir: String }` + `impl DeviceConfig`，写
      `data_dir/device_config.json`，格式与桌面一致；`file://` 前缀剥离照抄
      `app.rs:158-162` 的 `open_db`
- [x] 4.2 `mobile-core/src/lib.rs` 声明新模块；`mobile-core/src/app.rs` 的 `MobileCore` 增
      `device_config: Arc<MobileDeviceConfig>` 字段与 `pub(crate)` 访问器（用已有的
      `data_dir` 构造，构造函数签名不变）
- [x] 4.3 `mobile-core/src/network.rs:194` 删 `device_name: Option<String>` 入参；
      `:216` 改 `OsInfo::native()`；向 `start_node` 传 device_config 端口
- [x] 4.4 `mobile-core/src/pairing.rs:100` 删 `&OsInfo::default()` 实参 —— **移动侧 bug fix**
      （移动端邀请 display_name 此前恒为 "Device"）
- [x] 4.5 `mobile-core/src/device.rs`（或 `identity.rs`）新增 uniffi 导出
      `get_device_name() -> FfiResult<Option<String>>` /
      `set_device_name(name: Option<String>) -> FfiResult<()>`，后者内部 `DeviceName::parse`
- [x] 4.6 `mobile/` 下 `pnpm --filter react-native-swarmdrop-core build:ios`（或对应 android
      目标）重建 uniffi 桥接，确认 TS bindings 出现两个新方法、`startNode` 少一个参数
- [x] 4.7 `mobile/src/stores/mobile-core-store.ts:200-201`：`core.startNode(...)` 去掉
      `prefs.deviceName` 实参
- [x] 4.8 `mobile/src/stores/mobile-core-store.ts`：身份就绪后做**一次性迁移** ——
      `core.getDeviceName()` 为 null 且 `prefs.deviceName` 非空 → `core.setDeviceName(mirror)`；
      随后把 core 的值回写镜像
- [x] 4.9 `mobile/src/lib/device-name.ts:34-50` 的 `applyDeviceName` 改为先
      `core.setDeviceName(trimmed || null)` 再重启，返回 `{ saved, restarted }`（design D7）
- [x] 4.10 `mobile/src/components/device-info-card.tsx:58` 与
      `mobile/src/app/onboarding/device-name.tsx:40` 按新返回值分别提示；
      `device-info-card.tsx` 的输入框补 `maxLength={40}`（与 onboarding 对齐）
- [x] 4.11 `mobile/src/stores/preferences-store.ts` 的 `deviceName` doc 注释改成
      「core 的显示镜像，事实源在 Rust 侧 `device_config.json`」

## 5. Web 实现（`crates/web`）

- [x] 5.1 新增 `crates/web/src/device_config.rs`：`IdbDeviceConfig`（零尺寸）+
      `impl DeviceConfig`，读写 `idb::KV_STORE` 下新键
      `swarmdrop.deviceName.v1`；`JsFuture` 用 `SendWrapper` 裹（照
      `crates/web/src/invite_store.rs:42-60`）
- [x] 5.2 `crates/web/src/lib.rs` 加 `#[cfg(wasm_browser)] mod device_config;`
- [x] 5.3 `crates/web/src/device_config.rs` 新增三个**模块级** wasm 导出
      （不挂 `WebNode` —— 节点起不来时设置页仍要能改名，design D5）：
      `get_device_name() -> Option<String>` / `set_device_name(name: Option<String>)` /
      `default_device_name() -> String`（返回 `web_os_info().hostname`，供设置页 placeholder
      与对外表示同源，避免在 TS 里重写一份 UA 判定）
- [x] 5.4 `crates/web/src/node.rs`：`WebNode::spawn` 向 `start_node` 传
      `Arc::new(IdbDeviceConfig)`
- [x] 5.5 `crates/web/src/node.rs:134` 删 `os_info` 字段；`generate_invite`（:327-338）
      去掉第三个实参。`web_os_info()`（:675-706）**逻辑保持不动**，仅由私有提为
      `pub(crate)` 供 5.3 的第三个导出复用（design D5）
- [x] 5.6 `pnpm build:wasm`（在 `docs/` 下）重新生成 `docs/packages/swarmdrop-web`，
      确认 `.d.ts` 里出现三个新导出

## 6. Web 前端（`docs/app/app`）

- [x] 6.0 `docs/app/app/_lib/node-runtime.ts`：把私有的 `loadModule`(:15) 开成
      `export function getModule(): Promise<SwarmdropWebModule>`。**这是 D5 的实现前提** ——
      今天前端只拿得到 `WebNode` 实例，够不到任何模块级导出（核实记录 7）。
      注释写明它与 `spawnNode()` 共用 `modulePromise`，节点 spawn 失败不影响模块可用
- [x] 6.1 `docs/app/app/_lib/store.ts`：`WebNodeState` 加 `deviceName: string | null`
      （用户设的名字，null = 未设）与 `deviceNameFallback: string | null`（UA 派生的浏览器名，
      作 placeholder）；`webNodeActions` 加对应 setter。selector 只返回原始值 —— 自研 store
      同样有「派生新数组/对象 → 无限重渲染」陷阱，且 `pnpm check:zustand-access` 不扫 `docs/`
- [x] 6.2 `docs/app/app/_components/web-node-bootstrap.tsx`：经 `getModule()` 调一次
      `get_device_name()` + `default_device_name()` 灌进 store。**只在 layout 这一处做**，
      不要下放到 page；且**不要挂在 spawn 成功之后** —— 改名能力不依赖节点状态（spec 有一条
      「节点未运行时改名」的场景），节点起不来时设置页照样要显示当前名字
- [x] 6.3 `docs/app/app/_components/node-panel.tsx`：在「本机节点」块加设备名行 ——
      展示态 + 编辑态（Input `maxLength={40}` + 保存/取消），placeholder 用
      `deviceNameFallback`（= `default_device_name()`，**不在 TS 里另解析 UA**），
      保存调 `set_device_name` 并更新 store
- [x] 6.4 同文件：保存成功后给一句「刷新页面后对端才会看到新名字」的提示（本 change 的
      过渡语义，C6 会消除），并给一句说明当前默认值来自浏览器 UA（design D6）
- [x] 6.5 确认 `node-panel.tsx` 仍是 `"use client"`，且无新增动态路由段 / 未包 Suspense 的
      `useSearchParams`（静态导出三限制）

## 7. 桌面前端（`src/`）

- [x] 7.1 `src/lib/device-name.ts:24-38` 的 `applyDeviceName` 返回
      `{ saved: true, restarted: boolean }`，删掉吞掉重启失败的 `console.warn`（design D7）
- [x] 7.2 `src/routes/_app/settings/-device-info-section.tsx:92-103` 按新返回值分别提示；
      :183 的 Input 补 `maxLength={40}`（与 onboarding `device-name.lazy.tsx:112` 对齐）
- [x] 7.3 `src/routes/_onboarding/device-name.lazy.tsx:57` 同步适配新返回值
      （onboarding 阶段节点尚未启动，`restarted` 恒为 false，不应因此报错）
- [x] 7.4 i18n：新增/改动的串跑 `pnpm i18n:extract`，补 en / zh-TW 译文（**零空译**）

## 8. 顺带清理：`paths.rs` 接线与文档

- [x] 8.1 `src-tauri/src/host.rs` 加 `pub mod paths;`（放在 `#[cfg(debug_assertions)]` 之外 ——
      模块内部已自带 `cfg(debug_assertions)` 门控）
- [x] 8.2 `src-tauri/src/host/device_config.rs:25` 改用 `crate::host::paths::app_data_dir(app)`
- [x] 8.3 `src-tauri/src/host/file_keychain.rs:50` 改用 `crate::host::paths::app_data_dir(app)`
- [x] 8.4 `src-tauri/src/database.rs:24` 改用 `crate::host::paths::app_local_data_dir(app)`
- [x] 8.5 `e2e/desktop/demo-asset-plan.md:69` 那段改写：说明该覆盖此前从未生效
      （`mod paths` 未声明），本 change 起真正接线；`§6` 的隐私前提在此之前并不成立
- [ ] 8.6 **人工验证**：不设 `SWARMDROP_DATA_DIR` 起一次 `pnpm tauri dev`，确认 identity /
      `device_config.json` / SQLite 落点与接线前一致（身份未变、配对未丢）
- [ ] 8.7 **人工验证**：设 `SWARMDROP_DATA_DIR=e2e/desktop/build/demo-profile` 跑 seeder
      再起 app，确认设备列表是 fixture 里的假设备而非真实配对设备

## 9. 测试

> 分段验证的理由见 design D11：`start_node` 在 core 的测试里不可达（e2e 全部手抄了它的
> body），所以链路按「端口 → OsInfo」与「OsInfo → 三条对外表示」两段各自落在已有 harness 上，
> 中间那一行赋值交给编译期 + 人工验收（10.10 / 10.11）。

- [x] 9.1 `crates/host`：`DeviceName::parse` 的边界单测（1.2）与 agent_version 注入回归（1.3）
- [x] 9.2 `crates/core`：注入 `OsInfo` → `encode_invite` 的 `display_name` 等于注入值（2.12 ②）
- [x] 9.3 `crates/core`：`request_pairing` 构造的 `PairingRequest.os_info.name` 等于注入值
      （直接钉 `manager.rs:287` 这条 bug fix，防回归）（2.12 ①）
- [x] 9.4 桌面：`DesktopDeviceConfig` 的读写往返 + 「json 被手改成含 `;` 的名字时 load 侧
      仍归一化」两条
- [x] 9.5 移动：`MobileDeviceConfig` 的读写往返 + `file://` 前缀剥离
- [x] 9.6 `pnpm test`（仓库根 vitest）—— 覆盖 `applyDeviceName` 新返回形状的调用方

## 10. 门禁与验收

- [x] 10.1 `cargo fmt --all`
- [x] 10.2 `cargo check --workspace --all-targets`
- [x] 10.3 `cargo test --workspace`
- [x] 10.4 `cargo clippy --workspace`
- [x] 10.5 `./scripts/check-wasm.sh` —— **不可跳过**：`crates/web` 在 native target 下近乎
      空 crate，`cargo check --workspace` 抓不到 Web 端因 `start_node` / `encode_invite`
      签名变更产生的漏改
- [x] 10.6 `./scripts/check-wasm.sh --clippy`
- [x] 10.7 `pnpm test`、`pnpm exec tsc --noEmit`、`pnpm check:zustand-access`（本 change 动了
      仓库根 `src/`，跑）
- [x] 10.8 `mobile/` 下 `pnpm typecheck`
- [x] 10.9 `docs/` 下 `pnpm build`（静态导出必须过）
- [ ] 10.10 **人工 — 桌面**：设置页改名 → 生成邀请，确认邀请卡上的 display_name 是新名字
      （此前是 hostname）
- [ ] 10.11 **人工 — 跨端**：Web 端设名「书房 Chrome」→ 用桌面消费其邀请，确认桌面的配对
      确认弹窗显示「书房 Chrome」而非「Device · unknown」
- [ ] 10.12 **人工 — 移动升级路径**：用带存量 AsyncStorage 设备名的旧版本升级到本版本，
      确认设备名未丢（迁移生效），而非只验全新安装
- [ ] 10.13 **人工 — Web 持久化**：设名 → 刷新页面 → 名字仍在，且新生成的邀请带新名字
- [ ] 10.14 **人工 — 过渡限制确认**：已配对的对端在本机改名后**不会**立刻更新显示，
      重启节点（Web 刷新页面）后才更新 —— 这是本 change 的已知限制，spec 已写明，C6 消除

## 11. 收尾

- [x] 11.1 更新 `dev-notes/knowledge/rust-backend.md`：端口层现有 trait 清单（加
      `DeviceConfig`、删 `AppPaths`）、`DeviceName` 归一化是唯一入口、agent_version 分隔符
      注入这条坑
- [x] 11.2 更新 `CLAUDE.md`：`crates/host` 一行的职责描述若提到端口数量/清单，同步
- [ ] 11.3 GitHub issue #103 关联本 change，并在 issue 里注明「已配对对端看到新名字」这条
      验收标准由 C6 兑现
