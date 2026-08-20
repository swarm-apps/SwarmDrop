## Why

**邀请泄露后唯一的止损手段，恰恰在最需要它的时刻不可用。**

邀请 TTL 24 小时、跨重启存活。发现某张邀请泄露（截图、投屏、日志、旁人抢先扫码）时，唯一
有效的处置是打开清单把它撤掉。但三个 GUI 宿主的邀请清单**全部绑在运行中的节点上**：

| 宿主 | 现状 | 节点未运行时 |
|---|---|---|
| 桌面 | `list_pair_invites` 走 `with_manager!` | 抛 `NodeNotStarted` |
| 移动 | `list_pair_invites` 走 `pairing_manager().await?` | 报错 |
| Web | `list_invites` 是 `WebNode` 的方法 | 没有节点就没有这个对象 |

而记录**明明就在库里**——`invite` 表（native）/ IndexedDB（Web）里躺着，只是没人去 `load`。

桌面前端还把这件事误读成了「本来就没有」。`src/components/pairing/sent-invites.tsx`：

```ts
// 节点没启动时注册表本就是空的，这不是错误——静默当空列表处理。
if (!isErrorKind(err, "NodeNotStarted")) { toast.error(...) }
setInvites([]);
```

**「注册表本就是空的」是错的。** 于是用户在节点未启动时看到的不是「请先启动节点」，而是
一份理直气壮的空清单——它主动告诉用户「你没有任何邀请在外面」，而这正好是需要止损时最
危险的一句谎话。

设备列表有同一个毛病但后果轻些：桌面 `list_devices` 在节点未启动时直接 `node_not_started`，
设备页空白，观感是「我的设备都没了」。已配对设备表就在 `paired-devices.json` 里，节点停了
它照样在。

`cli-command-surface` 已让 CLI 做对了这两条（无节点时直连持久化记录，且设备的在线状态如实
呈现为「未知」而非猜测值）。**本 change 把其余三端追平**，消除那处有意但不该长期存在的分叉。

## What Changes

- **邀请清单与撤销在节点未运行时可用**（三端）。读取路径不再要求 `NetManager` 在场：
  建一个只依赖持久化端口的注册表读回记录即可——CLI 已验证这条路径可行且**不需要给
  `crates/invite` 加任何 API**（`InviteRegistry::new(store)` + `load(now)` 就是节点启动时
  做的事）。
- **已配对设备列表在节点未运行时可用**（三端）。在线状态在无节点时 SHALL 呈现为「未知」，
  MUST NOT 呈现为「离线」——那是一个未经探测的猜测值。
- **修正桌面前端把 `NodeNotStarted` 当空列表吞掉的处理**：该分支在读取不再依赖节点之后
  失去存在理由，留着会掩盖真实错误。
- **Web 端改造最重**：`list_invites` / `revoke_invite_by_id` 目前是 `WebNode` 的方法，
  需要把「读持久化记录」这条路径从节点实例上摘下来，使其在节点未 spawn 时仍可调用。
  形态待 design 定：可能是模块级函数（对齐 `getModule()` 已有的「纯派生不需要节点」先例），
  也可能是一个独立的轻量句柄。

**非目标**：不改动写路径的节点依赖——生成邀请、发起配对、发送文件本就需要网络，
它们要求节点在场是正确的。本 change 只解决**读与撤销**。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `invite-lifecycle`: `### Requirement: 发起方可见并可撤销已发出的邀请` 目前未规定节点前提，
  因此现有实现不算违规、但也不受任何保证约束。需加强为「该能力 SHALL 在节点未运行时同样
  可用」，并补一条对应 scenario。

其余受影响的行为（设备列表、桌面前端的错误吞并）落在哪个 capability 需在 specs 阶段确认：
`device-commands` 描述的是桌面 Tauri 命令的形态，可能需要一条 MODIFIED；也可能更适合归入
一条跨宿主的新 requirement。**该判断刻意留到动手时再做**，避免现在凭空造一个 capability。

## Impact

**桌面**
- `src-tauri/src/commands/pairing.rs`：`list_pair_invites` / `revoke_pair_invite_by_id`
  脱离 `with_manager!`
- `src-tauri/src/commands/lifecycle.rs`：`list_devices` 的 `node_not_started` 分支
- `src/components/pairing/sent-invites.tsx`：删掉把 `NodeNotStarted` 当空列表的分支
- 设备页在节点未运行时的呈现（含在线状态「未知」态）

**移动**
- `mobile/packages/swarmdrop-core/rust/mobile-core/src/pairing.rs`：`list_pair_invites` /
  `revoke_pair_invite_by_id` 脱离 `pairing_manager()`
- 对应的 RN store 与界面

**Web**
- `crates/web/src/node.rs`：读路径从 `WebNode` 摘出（改动形态见 What Changes）
- `docs/app/app/_components/pairing-panel.tsx` 与相关 store
- ⚠️ **wasm 产物需重新生成并入库**（`cd docs && pnpm build:wasm`），否则线上停在旧代码

**不修改**
- `crates/invite` / `crates/core`：CLI 已验证无需新 API

**前置**：`cli-command-surface` 先落地——它是这条路径的第一个实现，也是它的实测依据。
本 change 不必等它归档，但应在其实现完成后再动手，以便直接复用已验证的形态。
