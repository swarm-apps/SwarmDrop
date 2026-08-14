# device-config-port 设计

闭合根因 R3 的另一半：端口层缺设备配置域。顺带修掉「用户设的名字进不了配对请求，也进不了
邀请串」这条三端都中的存量 bug。

依赖 `C4 atomic-unpair-and-paired-device-store`：两者同改 `crates/host/src/ports.rs` 与
`start_node` 的签名，**是合并顺序上的依赖，不是语义依赖** —— C5 不消费 `PairedDeviceStore`，
C4 也不消费 `DeviceConfig`。若 C4 延期，本 change 可独立落地，只是 `ports.rs` 与组合根会与
C4 产生文本冲突。

---

## 核实记录：与初始描述不符的地方

写 design 前逐条核过给定的 file:line，下面几条与描述有出入，以代码为准：

1. **Web 端不是「三处 `OsInfo::default()`」之一。** `WebNode::generate_invite`
   （`crates/web/src/node.rs:327-338`）传的是 `&self.os_info`，即 `web_os_info()` 的真值 ——
   **Web 发出的邀请早就带着「Chrome / web」**，没有这条 bug。Web 中的是另一半：
   `request_pairing`（`crates/core/src/pairing/manager.rs:287`）在 core 里，三端共用，所以
   Web 发起的配对请求在对端弹窗上显示成「Device · unknown」。三处 `OsInfo::default()` 的
   实际分布是：core 1 处（三端共享）+ 桌面 1 处 + 移动 1 处。
2. **`os_info` 的影响面比「邀请 + 配对请求」更宽一格。** `PairingRequest.os_info` 还喂了
   入站系统通知（`manager.rs` handle_inbound 取 `req.os_info.hostname`）。修 bug 时那一处
   要一并改成 `display_name()`，否则「弹窗显示用户名 / 通知显示 hostname」自相矛盾。
3. **「空名 / 超长名的约束对齐桌面」这句话没有对象可对齐 —— 桌面自己就不一致：**
   onboarding 有 `maxLength={40}`（`src/routes/_onboarding/device-name.lazy.tsx:112`），
   设置页的 Input 没有任何上限（`src/routes/_app/settings/-device-info-section.tsx:183`），
   后端 `set_device_name` 只做 trim + 空串归 `None`（`src-tauri/src/commands/identity.rs:60`）。
   所以本 change 要先**定**一个约束再谈对齐，见 D3。
4. **`paths.rs` 不只是死代码，是一个失效的隐私开关。** 见 D8 —— 结论从「删或接线二选一」
   收敛到「只能接线」。
5. **`AppPaths` 的死法比预期彻底**：唯一 impl 是测试替身 `MemoryHost`
   （`crates/core/src/host.rs:258`），唯一调用点在 `#[cfg(test)] mod tests`（:558）。
   零生产实现、零生产消费、零 IPC 暴露（`CoreAppPaths` 不在 `src/lib/bindings.ts` 里）。见 D9。
6. **`start_node` 在 core 的测试里根本不可达。** `crates/core/tests/e2e_transfer.rs:116`
   的注释写得很直白：「复刻 `runtime::start_node` 的 body，但换成 `test_endpoint`」——
   三份 e2e（`e2e_transfer` / `e2e_lan_helper` / `presence_lifecycle`）与 `infra_reconcile`
   全都直接调 `NetManager::new` + `build_router`，**绕过组合根**。后果有两条：
   (a) 本 change 在 `start_node` 里新加的「从端口填 `os_info.name`」不会被这套 e2e 覆盖，
   验证点必须另放（见 D11）；(b) 这份手抄 body 是既存的漂移源 —— 本 change 只是又给它加了
   一行不同步的内容，不在此扩大范围去消除它。
7. **Web 前端够不到模块级 wasm 导出。** `docs/app/app/_lib/node-runtime.ts` 只导出
   `spawnNode` / `getNode` / `closeNode`，`loadModule`（:15）是私有的。D5 选「模块级导出而非
   `WebNode` 方法」时必须连带把它开出来，否则设置页拿不到那两个函数 —— 这是 D5 的实现前提，
   不是可选优化。
8. **`MemoryHost::new` 的调用点不是 4 处，是约 30 处。** 给定描述里的四个位置
   （`e2e_transfer.rs:62` / `e2e_lan_helper.rs:38` / `identity.rs:162` / `database.rs:93`）
   是 `test_paths()` helper 的**定义**处；实际 `MemoryHost::new(test_paths())` 在
   `e2e_transfer.rs` 里就有 20+ 处。改动仍是机械的（删 helper + 全文替换），但工作量预期要摆正。

---

## D1：端口该放在 `crates/host`，且 core 组合根收它，而不是让 host 自己读完再传值

三种形态：

| 形态 | 谁读设备名 | 后果 |
|---|---|---|
| A. 不建端口，host 各自读完塞进 `OsInfo` 再传给 `start_node` | 每个 host | 就是今天。桌面 `lifecycle.rs:55` 读、移动从 JS 入参拿、Web 没有 —— 「三端各写各的」原封不动，只是被端口这个词包装了一下 |
| B. 建端口，但只给 host 层自己复用 | 每个 host | core 仍然拿不到设备名，`PairingManager` 的 `OsInfo::default()` 还得靠另一条路修 |
| **C. 建端口，`start_node` 收 `Arc<dyn DeviceConfig>`，core 内部装配 `OsInfo`** | core | 本机 OsInfo 有了唯一装配点 |

选 C。理由是根因本身：bug 的成因不是「没人存设备名」（桌面移动都存了），而是**没有一个地方
能回答「本机 OsInfo 是什么」** —— `start_node` 收到过一份，用完 `agent_version` 就扔
（`crates/core/src/runtime.rs:105`），于是下游各自 `OsInfo::default()`。把读取动作搬进 core，
「平台探测 + 用户设备名 = 本机 OsInfo」这条装配就只发生一次。

**enforcement 不靠注释。** `start_node` 仍收一个 `os_info: OsInfo`（平台探测部分：hostname /
os / platform / arch），core 覆写它的 `name`。为了让「host 别自己填 name」不是一句口头约定，
把 `OsInfo::native(name: Option<String>)` 改成 `OsInfo::native()`：宿主端**没有** API 可以
注入名字。`web_os_info()` 里的 `name: None` 同理保留为字面量。

副产品：`WebNode` 的 `os_info` 字段（`crates/web/src/node.rs:134`）在 `encode_invite` 去掉
`display` 参后没有消费方了，直接删掉 —— 它本身就是「本机 OsInfo 的第二份副本」。

## D2：`load` 不返回错误，`save` 返回错误

```rust
#[async_trait]
pub trait DeviceConfig: Send + Sync {
    /// 读取持久化的设备名。**不返回错误** —— 见下。
    async fn load_device_name(&self) -> Option<DeviceName>;
    /// 写入设备名；`None` 清空回退到 hostname。
    async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()>;
}
```

不对称是刻意的，沿用桌面现有实现里已经验证过的语义
（`src-tauri/src/host/device_config.rs:7-9` 那段注释里唯一值得保留的部分）：

- **load 失败必须降级而不是冒泡**。它在 `start_node` 的启动路径上。一个手改坏的 JSON、
  一次 IndexedDB 打不开，如果能让 `start_node` 返回 `Err`，代价是「节点起不来」——
  而正确行为显然是「用 hostname 兜底继续跑」。把这个降级放进 trait 契约，比放在每个调用点
  的 `.unwrap_or_default()` 里更难被后来者写反。
- **save 失败必须冒泡**。它只在用户点保存时发生，静默失败等于「改了名字，重启后又变回去」，
  且没有任何信号。桌面今天已经是这样做的（`commands/identity.rs:62` 把 io::Error 包成
  `AppError`）。

wasm 侧：`#[async_trait]` 默认要求 future 是 `Send`，Web 实现按 `IdbInviteStore`
（`crates/web/src/invite_store.rs:17,42`）的成例用 `SendWrapper` 裹 `JsFuture`。
单线程 wasm 下跨线程 panic 永不触发，这条已在 `storage-abstraction.md` 里定过。

## D3：约束用 newtype 强制，不是靠每个调用点自觉

`DeviceName::parse(&str) -> Option<DeviceName>`，是唯一构造入口：

1. `trim()`
2. 剥控制字符与 `;`
3. 截断到 40 个 **char**（不是 byte —— 中文名 40 字要占 120 字节）
4. 结果为空 → `None`（= 清空，回退 hostname，正是端口 `Option` 的语义）

选项对比：

| 方案 | 成本 | 漏网概率 |
|---|---|---|
| 自由函数 `normalize_device_name()` + 各调用点自觉调用 | 最低 | 高。要在桌面命令、移动 uniffi 导出、wasm 导出、以及未来任何新入口各调一次；漏一个就退回今天 |
| trait 提供 provided method 包一层 required method | 中 | 低，但 trait 长出两层方法名，实现者容易实现错那个 |
| **newtype + 智能构造函数** | 约 30 行 + 单测 | 结构性为零 —— 端口签名吃 `Option<DeviceName>`，未归一化的 `String` 编译期就传不进去 |

选 newtype。FFI / IPC 边界仍用 `Option<String>`（uniffi 与 specta 都不必认识这个类型），
`DeviceName` 只活在 Rust 内部：`get_device_name` 返回 `Option<String>`，`set_device_name`
收 `Option<String>` 后立刻 `parse`。

**为什么剥 `;` 属于本 change 而不是「顺带洁癖」**：`OsInfo::to_agent_version()`
（`crates/host/src/device.rs`）把字段拼成 `swarmdrop/x.y; name={n}; caps=a,b; os=…`，
`from_agent_version()` 按 `"; "` 切片再按 `name=` / `caps=` 前缀分派。设备名里带一个
`"; caps=lan-helper"` 就能让对端解析出一个本机并不具备的 capability，而
`crates/core/src/network/event_loop.rs:123` 正是靠 `has_capability(LAN_HELPER_CAPABILITY)`
决定要不要把这个 peer `add_infrastructure_peer(kad_server: true, relay: true)`（:150-165）。

范围有限（同局域网、需 `auto_discover_lan_helpers`、结果是被当成 kad/relay 候选而非拿到
任何数据），但它今天在桌面/移动上**已经可触发** —— 用户本来就能设任意设备名。本 change
把设备名做成「三端共用一个归一化入口」，顺手把这条一起关掉是零边际成本；不关反而是明知故犯
（Web 端加入后又多一个可设名字的端）。

对齐动作（因为桌面自己不一致，见核实记录 3）：**40 字符上限由 newtype 兜底，三端 UI 一律
`maxLength=40`**。截断而非报错 —— UI 已经拦在前面，后端截断只是防御纵深，为它造一条跨三端的
错误路径不划算。

## D4：移动端把设备名挪进 Rust 侧落盘，不走 uniffi callback

两条路：

| | A. Rust 侧写 `data_dir/device_config.json` | B. 新增 `ForeignDeviceConfig` uniffi callback → JS AsyncStorage |
|---|---|---|
| 新增 FFI 面 | 无（只加两个普通导出） | 一个 `#[uniffi::export(with_foreign)]` trait + 适配器，形如 `keychain.rs:14-25` |
| 与桌面的对称性 | 完全对称：同名文件、同格式、同降级语义 | 不对称：桌面是文件、移动是 JS |
| 存量迁移 | 需要一次性把 AsyncStorage 的值推下去 | 不需要 |
| 启动期依赖 | 无 | `start_node` 装配途中要跨桥回 JS 一次 |
| 移动端 prefs 的完整性 | 设备名离开 `preferences-store` 的持久化面 | 所有 prefs 仍在一处 |

选 A。决定性理由是 R3 的目标本身：B 只是把「移动端设备名归 JS 管」这个事实换了个更正式的
写法，Rust 侧依然要靠外部喂；而 A 之后三端的实现体量都是「一个文件 / 一个 KV，读写 +
降级」，`DeviceConfig` 这个端口才真的在收敛差异。配套条件也已就位：`MobileCore::new` 已经
持有 `data_dir`（`app.rs:29,44`），`open_db`（`app.rs:154-163`）里连 `file://` 前缀剥离都
写好了，照抄即可，零新增管道。

代价是迁移，且必须做 —— 漏掉等于「升级后设备名被清空」。做法：JS bootstrap 里
`getDeviceName()` 返回 null 且本地镜像非空时推一次 `setDeviceName(mirror)`。
`preferences-store.deviceName` 保留为**显示镜像**（与桌面 `usePreferencesStore` 的角色一致，
见 `src/lib/device-name.ts:27,48`），不再是事实源。

## D5：Web 的 UA 派生结果不需要改，它本来就在「默认值」那个字段上

issue #103 要求「UA 派生结果降级为默认值而非唯一值」。核过代码后这条不需要新增降级逻辑：

- `web_os_info()`（`crates/web/src/node.rs:675-706`）把浏览器名写进 **`hostname`**，
  `name` 是 `None`。
- `OsInfo::display_name()`（`crates/host/src/device.rs`）本来就是
  `name.trim() 非空 ? name : hostname`。

所以「Chrome」今天就已经处在默认值的位置上，缺的只是 `name` 有来源。本 change 让
`start_node` 从 `IdbDeviceConfig` 填 `name`，UA 派生逻辑一行不动。
设置页的输入框把 `hostname` 当 placeholder 展示，用户清空输入即回落到它 —— 与桌面
「清空回退 hostname」的语义一字不差。

**导出三个而不是两个。** placeholder 要显示的那个浏览器名，前端得有地方拿。两条路：在 TS 里
再解析一次 UA，或者把 Rust 已经算好的值导出来。选后者 —— 前者会造出第二份 UA 判定表，而
「设置页 placeholder 写 Safari、对端看到 Browser」这种不一致既难发现又完全没有价值。于是
`web_os_info()` 提到 `pub(crate)`，加第三个导出 `default_device_name() -> String` 返回它的
`hostname`。桌面/移动不需要这一条：它们的默认值是系统 hostname，前端本来就能从
`tauri-plugin-os` / `expo-device` 拿到。

wasm 导出选 **模块级自由函数**而不是 `WebNode` 方法：设置页在节点 spawn 失败（`status:
"error"`）时仍然可达，改名不该被节点状态绑架。`docs/app/app/_lib/view-types.ts:27` 的
`SwarmdropWebModule = typeof import("swarmdrop-web")` 会自动带上新导出的类型，前端零手抄。

实现前提（核实记录 7）：`node-runtime.ts` 的 `loadModule`(:15) 要开成 `getModule()`。
它与 `spawnNode()` 共用同一个记忆化 Promise，所以「节点起不来但模块已加载」正是我们要的状态 ——
`spawnNode()` 失败时清的是 `spawnPromise`，`modulePromise` 不受影响。

## D6：Web 不做强制命名引导（issue #103 的待定项，本 change 定掉）

三个选项：

- **强制引导**（照搬桌面 onboarding）：与 Web 的产品前提「点开链接就能用」正面冲突。
  桌面装一次用很久，一次性提问摊得起；Web 的典型入口是别人发来的邀请链接，此时插一个
  必填步骤，用户还没看到任何价值就先被要求填表。
- **软引导**（首次进 `/app/devices` 弹一次非模态提示条）：折中，但它要新增「是否提示过」
  的持久化状态，而收益只在「用户恰好在第一次就想改名」这一格上。
- **不引导，设置页可改**：默认值（浏览器名）立刻可用，想区分的人自己去改。

选第三条。判据是这条能力的**失败代价**：不改名的后果是对端列表里出现一行「Chrome」——
可读、不阻断、随时可补救；而强制引导的代价发生在每一个首次访问者身上。两者不对称。

配套：Web 设置页的设备名区块给一句说明（当前值来自浏览器 UA、对端看到的就是这一行），
让「可改」这件事被发现，而不是靠用户猜。

## D7：改名与重启是两件事，UI 分别反馈

现状两份 `applyDeviceName` 已经分叉：

- `src/lib/device-name.ts:29-37`：重启失败只 `console.warn`，调用方
  （`-device-info-section.tsx:96`）照样 `toast.success("设备名称已更新")` —— 名字确实存了，
  但**节点已经停了**，用户看到的是成功提示。
- `mobile/src/lib/device-name.ts:38-49`：`throw`，调用方吞进 catch 报错 —— 但名字其实已经
  保存成功了，提示却是纯失败。

两边都在用一个布尔结果表达两件独立的事。改成返回 `{ saved: true, restarted: boolean }`，
调用方按 `restarted` 决定提示语（「已更新」/「已保存，但节点重启失败，请手动重启」）。

**不抽公共代码** —— `src/`、`mobile/`、`docs/` 是三个独立 pnpm workspace，共享要新起一个包，
为 15 行逻辑不值。对齐的是语义与返回形状，不是文件。

Web 没有 restart 这一步：改名后新值要等下一次 `WebNode::spawn`（= 刷新页面）才进
`agent_version` 与 `PairingManager` 的快照，所以 Web 的提示直接写「刷新页面后生效」。

## D8：`paths.rs` 接线，不删

初始判断是「删除或接线二选一」。核完之后只剩接线一条路：

- 它不是无人认领的死代码。`e2e/desktop/demo-asset-plan.md:69` 明确宣传它，录制流程
  （:77-79、:100-101、:128）按那份文档传 `SWARMDROP_DATA_DIR`。
- seeder `crates/core/examples/seed_demo_profile.rs:52` **认这个变量**，会把假身份写进 fixture 目录。
- 而 app 不认（`mod paths` 从未声明），三个调用方直读 Tauri API：`device_config.rs:25`、
  `file_keychain.rs:50`、`database.rs:24`。

于是当前状态是：seeder 往 fixture 写、app 从真实 profile 读，**录出来的演示素材带的是真实
设备名与 peer ID**，正好违反同一份文档 §6 的隐私约束。删掉 `paths.rs` 只是把「坏了」改写成
「没有」，那条隐私要求还是没人满足。接线的成本是一行 `pub mod paths;` + 三处调用点改成
`crate::host::paths::app_data_dir(&app)?` / `app_local_data_dir(&app)?`，且整段逻辑
`cfg(debug_assertions)` 门控，release 行为按定义不变。

顺带修 `demo-asset-plan.md`：把 §3.1 那段从「已实现」改成描述接线后的真实行为。

风险见 proposal：开发机上若已设 `SWARMDROP_DATA_DIR`，接线后 debug build 的三处存储会一起
搬家（表现为身份变了、配对全丢）。这是**期望行为**，但要在未设变量的干净环境里确认默认落点
与今天一致。

## D9：`AppPaths` 端口删除

`crates/host/src/ports.rs:75` 的 `AppPaths` 与 `CoreAppPaths`：

- 唯一实现：`crates/core/src/host.rs:258` 的 `MemoryHost`（测试替身）。
- 唯一调用：`crates/core/src/host.rs:558`，在 `#[cfg(test)] mod tests` 里，断言的是
  「MemoryHost 返回构造时传进去的路径」—— 一条只测试自己的测试。
- 桌面明确不需要它，`src-tauri/src/host.rs:10-12` 写着理由（接收目录由用户在 acceptReceive
  时显式传入）。移动 / Web 从未提供实现。
- 不在 IPC 契约上（`CoreAppPaths` 未出现在 `src/lib/bindings.ts`）。

一个零实现、零消费的端口 trait 的害处不是占空间，是它让端口层的覆盖率看起来比实际高 ——
本 change 的整个论证前提是「数一数端口层有什么」，而这一格是假的。删除连带：
`MemoryHost::new(paths)` → `MemoryHost::new()`，改到 `crates/core/tests/e2e_transfer.rs:62`、
`crates/core/tests/e2e_lan_helper.rs:38`、`crates/core/src/identity.rs:162`、
`src-tauri/src/database.rs:93` 四处测试构造。全部机械改动，编译期可查。

若将来要恢复「默认下载目录」，重建这个端口的成本是十几行；保留一个假实现的成本是持续误导。

## D10：`PairingManager` 持不可变 `OsInfo` 快照，可变化留给 C6

`PairingManager` 增一个 `os_info: OsInfo` 字段（组合根注入），`encode_invite` 去掉
`display: &OsInfo` 参数、`request_pairing` 用 `self.os_info.clone()`。

要不要现在就做成可变（`RwLock<OsInfo>` / `ArcSwap`）？不。

- 本 change **没有写者**：改名后节点整体重启，新的 `PairingManager` 带着新的快照建起来，
  与 `agent_version` 同一时刻更新。此时加锁是给一个不存在的写路径预留结构。
- 更糟的是它会造出一个**中间态**：若邀请的 display 能热更新而 `agent_version` 不能，
  就会出现「新发的邀请写着新名字，对端 identify 到的还是旧名字」——两条对外表示不一致，
  比两条一起延后更难解释。
- C6 要引入的是「net actor 收命令改 agent_version」那条通路，届时本机 OsInfo 的可变句柄
  形状由那条通路的需要决定（要不要 watch、谁是所有者）。现在猜一个形状，C6 大概率要改。

所以本 change 的语义是明确的：**本机 OsInfo 在节点生命周期内不变**。这条同时是 spec 里
那条过渡限制的实现依据。

C6 会在这个字段上加写者（它的 `rename_device` 编排要调 `pairing.set_os_info`）。届时把
`os_info: OsInfo` 换成可变句柄是一处局部改动，且那时才有真实的读写并发形态可依据。

## D11：验证点放在「端口 → OsInfo」与「OsInfo → 三条对外表示」两段，不放 `start_node`

核实记录 6 说明了原因：core 的 e2e 全部绕开 `start_node` 自己拼装（`e2e_transfer.rs:116`
明写「复刻 body」），所以「给 `start_node` 传一个 stub `DeviceConfig` 然后断言」这条路
既没有现成 harness，写出来也只覆盖一条真实调用方不走的路径。

改成按段验证，每段落在已有 harness 上：

| 段 | 落点 | 断言 |
|---|---|---|
| 归一化 | `crates/host/src/device.rs` 单测 | `DeviceName::parse` 的四条边界（D3） |
| `OsInfo` → agent_version | `crates/host/src/device.rs` 单测 | 含 `;` 的原始串经 parse 后往返 `to_agent_version` / `from_agent_version`，`capabilities` 为空（D3 的注入回归锚点） |
| 注入 → 配对请求 | `crates/core` 内，构造 `PairingManager` 时直接注入一个 `OsInfo` | `request_pairing` 发出的 `PairingRequest.os_info.name` == 注入值。这条直接钉 `manager.rs:287` 那个 bug |
| 注入 → 邀请串 | 同上 | `encode_invite` 产出的邀请 decode 后 `display_name` == 注入值 |
| 端口 → 注入 | 三端各自的 adapter 单测（D2 的读写往返） | 端口本身的行为 |

中间那一跳（`start_node` 把端口读出来的名字放进 `os_info`）留给编译期 + 人工验收
（tasks 10.10 / 10.11）—— 它是一行赋值，为它造一套组合根 harness 不划算；而
「装配漏了」的表现是人工验收第一步就能看见的「邀请上还是 hostname」。

---

## wasm 三条硬约束的触碰情况

| 约束 | 本 change 是否触碰 | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **否** | 新增依赖只有 `Arc<dyn DeviceConfig>`，端口签名里只有 `Option<DeviceName>`（内部是 `String`）与 `AppResult<()>`。`start_node` / `NetManager::new` / `PairingManager::new` 的新参数不引入任何 sea-orm 类型 |
| **`crates/transfer` 零 network 依赖** | **否** | `crates/transfer` 一个文件都不改 |
| **`crates/invite` 零 core 依赖** | **否** | `PairInvite::generate` 的签名不变（仍收 `display_name: String` + `display_platform: String` 两个纯串）。变的是**调用方**：`encode_invite` 从「参数」改为「读 `self.os_info`」，这一步全在 `crates/core` 内 |

另外三条相关约束：

- **`crates/host` 必须 wasm-clean**（它在 `scripts/check-wasm.sh` 的 `CRATES` 列表里）。
  新增的 `DeviceConfig` 与 `DeviceName` 只用 `std` + `async_trait`，无 IO、无平台 API。
- **Web 端「只有接收方向能续传」** 与本 change 无关（不碰传输会话落库）。
- **版本号三处同步**：本 change 不发布，不涉及。**libp2p rev**：不升级，不涉及（那是 C6）。

必跑：`./scripts/check-wasm.sh` 与 `./scripts/check-wasm.sh --clippy` —— `crates/web` 在
native target 下近乎空 crate（`lib.rs:9-11` 的 `cfg(wasm_browser)` 门控），
`cargo check --workspace` **抓不到** Web 端因签名变更产生的漏改。
