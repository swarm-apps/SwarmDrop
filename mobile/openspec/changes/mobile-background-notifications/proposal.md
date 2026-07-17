## Why

桌面端 SwarmDrop 在收到配对请求或传输 offer 时会弹一条系统通知,长传时还需要一条实时进度提示。移动端目前有两个缺口叠加:(a) 现有本地通知虽已接线(`event-bus.ts:62/77` → `notifier.ts`),但 `expo-notifications` 未注册 handler、前台默认抑制,且**根本不支持进度条 / ongoing 通知**;(b) OS 挂起 app 后 `tokio`+`libp2p` 节点收不到事件,后台无任何通知、长传也会被系统掐断。

`expo-notifications` 无法覆盖进度条 / ongoing / 前台服务,而 Notifee(社区曾用来补这块)已于 2026-04-07 被 archive、停止维护——其官方 README 指向 `react-native-notify-kit`(社区维护 fork,drop-in 替代)。因此本次统一改用 `react-native-notify-kit` 承载全部通知,并借它的前台服务能力打通后台保活与传输进度展示。

## What Changes

- **将通知栈从 `expo-notifications` 迁移并统一到 `react-native-notify-kit`**,移除 `expo-notifications`;配对请求 / 传输 offer 告警前台直接展示(fork 前台事件投递,无 handler 抑制问题)。
- 通知点击深链:经 fork 的 action → JS 事件与已定义的 `swarmdrop` scheme 跳转到配对 / 传输界面(含冷启动)。
- 权限前置:`POST_NOTIFICATIONS` 从"事件到达时惰性申请"前移到 onboarding / 首次进设置预热,被拒时给可深链到系统设置的重开路径。
- 专用高优先级 Android 通知渠道,配置品牌通知图标 / tint(经 fork 的 Expo config plugin)。
- 新增 Android **前台服务**(用 fork 的 foreground service API,同进程,仅保活),让节点在后台 / 息屏后继续运行、对端可寻址,并支撑长传保活;生命周期与 `mobile-core-store` 的 `runtimeState` 严格绑定。
- 新增 **传输进度 ongoing 通知**:Android 上一条可实时 `update` 的常驻通知,含进度条、文件名 / 百分比 / 速率,以及 pause / cancel action(action 路由回 transfer manager);active transfer 期间由前台服务承载。
- 声明所需 Android 后台权限与类型化前台服务(大部分由 fork 的 config plugin 处理),并选定 `foregroundServiceType`。

非目标(明确边界):
- **iOS 深后台**(息屏 / 被杀)收配对不做,且不引入推送中继。架构性原因:iOS 挂起进程后 `libp2p` relay v2 预约(活电路 + TTL + keep-alive)几十秒内掉线、DCUtR 打洞连接死亡,对端无法再寻址,且 relay v2 无 store-and-forward,`BGTask` 唤醒也救不回来。iOS 只承诺"前台 / 刚退台短窗口可提醒,深后台不保证"。
- **iOS 传输进度不做锁屏 Live Activities**:iOS 系统通知无进度条能力,锁屏 / 灵动岛实时进度需 ActivityKit + SwiftUI widget extension(iOS 16+),属独立立项;本次 iOS 传输进度**仅在 app 内展示**。
- `autoStart` 保持 `false`(对齐桌面),本次不改。
- Rust core 不改动(零 FFI 变更);前台服务永不拥有节点。

## Capabilities

### New Capabilities

- `mobile-notification-delivery`: 经 `react-native-notify-kit` 的配对请求 / 传输 offer 告警展示、点击深链跳转、权限生命周期与 Android 通知渠道配置。
- `mobile-background-keepalive`: 用 fork 的前台服务在后台保持 P2P 节点进程存活且可寻址,生命周期绑定节点运行态,并明确平台边界(iOS 深后台不支持)。
- `mobile-transfer-progress-notification`: Android 上实时更新的传输进度常驻通知(进度条 + pause/cancel action),active transfer 期间由前台服务承载;iOS 仅应用内展示进度。

### Modified Capabilities

无。`openspec/specs/` 为空,不存在已归档的相关能力。

## Impact

- 依赖:移除 `expo-notifications`;新增 `react-native-notify-kit` 及其 Expo config plugin。
- `src/core/notifier.ts`:从 `expo-notifications` 迁移到 fork(告警展示、渠道创建、权限预热 + 被拒回退、action / response 事件)。
- `src/core/event-bus.ts`:现有 `fireNotify*` 接线保留;通知 payload 携带深链路由数据。
- `app.json`:移除 expo-notifications 相关,加入 fork 的 config plugin 与 Android 后台权限 / 前台服务类型。
- `src/stores/mobile-core-store.ts`:与 `runtimeState` 同步启停前台服务(节点运行 ⇔ 服务启用)。
- `src/stores/transfer-store.ts`:传输进度驱动 ongoing 通知的实时更新与启停。
- Onboarding / 设置界面:通知权限预热 + 可选的国产 OEM 电池优化引导。
