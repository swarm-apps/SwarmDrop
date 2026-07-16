## Context

- 通知链路已存在:Rust 事件 → `event-bus.ts` 的 `fireNotify*`(`:62/:77`)→ `notifier.ts`。但 `expo-notifications` 未注册 handler、前台被抑制,且**不支持进度条 / ongoing / 前台服务**(官方明确不覆盖)。
- 节点生命周期与 AppState 解耦(`_layout.tsx:62-64` 刻意不随退台 shutdown/start),`autoStart` 默认 `false`;节点是纯 `libp2p 0.56`(relay v2 + dcutr + autonat + kad + mdns + quic/tcp),`tokio` 运行时活在原生 turbo module 进程内,`net_manager` 存于 `MobileCore` 的 `Mutex`。
- 无任何后台执行能力(无 `UIBackgroundModes` / foregroundService / `<service>` / FGS 权限)。`_layout.tsx:64` 已把"长传保活"显式记为待办。
- **通知库现状**:`Notifee` 于 2026-04-07 被 archive(只读)、停止维护,最后版本 `9.1.8`(2024-12);其官方 README 指向两条后路——`expo-notifications`(不支持进度/ongoing/FGS)与 `react-native-notify-kit`(社区维护 drop-in fork)。
- `react-native-notify-kit`:Notifee 钦点继任者,活跃(162★ / 1,791 commit / 0 open issue),单人维护(Marco Crupi);支持 Expo CNG config plugin(自动处理 iOS NSE + Android FGS manifest)、前台服务(timeout 处理 + OEM 韧性)、进度指示器、action→JS 事件投递。API 与 Notifee 一致,迁移=import 改名。
- 已有 config plugin(`with-android-manifest` 注入权限、`with-app-build-gradle` 正则改 gradle)可作补充注入的模板。Expo SDK 56 走 prebuild,原生改动需经 config plugin 保证可复现。

## Goals / Non-Goals

**Goals:**
- 前台告警真正弹出并可点击深链;权限前置 + 被拒可回退;Android 渠道 / 图标品牌化。
- Android 前台服务实现后台 / 息屏保活(收配对 + 长传保活),节点持续可寻址,Rust 侧零改动。
- Android 传输进度常驻通知(进度条 + pause/cancel action),实时更新。
- 全部通知统一到 `react-native-notify-kit` 一条栈,移除 `expo-notifications`。

**Non-Goals:**
- iOS 深后台(息屏 / 被杀)接收配对——不做,不引服务器 APNs。
- iOS 传输进度锁屏 Live Activities(ActivityKit)——独立立项,本次 iOS 进度仅应用内。
- 改动 `autoStart` 默认值;修改 Rust core / FFI;用远程推送替代本地通知。

## Decisions

### D1: 全部通知统一到 `react-native-notify-kit`,移除 `expo-notifications`
一条通知栈承载告警 + FGS + 进度。fork 前台事件直接展示,天然没有 `expo-notifications` 的 handler 抑制问题(故原先"1 行 setNotificationHandler"不再适用——P0 变为 notifier.ts 的库迁移)。
- 备选 A:留 `expo-notifications` 做告警 + 自写 native 做 FGS/进度 —— 否决(用户选择统一);但记录其优点(零第三方通知依赖、first-party),作为 fork 万一失维时的回退方向。
- 备选 B:直接依赖 `@notifee/react-native` —— 否决,已 archive。
- 双栈并存(expo + fork)—— 否决,两套通知基建易冲突;consolidation 更干净。

### D2: 迁移 `notifier.ts` 到 fork,深链走 action→JS 事件
`notifier.ts` 改用 fork 展示告警;`content.data` 带 `{ kind, pendingId / sessionId }`;用 fork 的前台 / 后台事件监听(点击、action)+ 初始通知事件(冷启动)导航到配对 / 传输界面。两条前台通道(系统通知 + in-app 弹窗)对同一 pending 请求幂等。

### D3: 权限前置到 onboarding,被拒落到可跳设置的状态
新增轻量 permission 状态,onboarding / 首次进设置时 `requestPermissions`;被拒时设置界面暴露 `Linking.openSettings()`。`notifier.ts` 的惰性申请降为兜底。

### D4: 专用高优先级渠道 + 品牌图标经 fork 的 config plugin
`app.json` 加 fork 的 config plugin 配 icon/color;启动时建专用渠道,告警显式指定 channelId(fallback 渠道 importance/图标不可控)。

### D5: Android 前台服务用 fork 的 foreground service,不自写 Kotlin
用 fork 的 `registerForegroundService` + `asForegroundService` 通知承载保活;喂一个 pending 不 resolve 的 runner 保活 native tokio 线程(FGS 是进程级构造,JS runner 空转即可,真实工作在 native)。**同进程**(无 `:remote`),服务只举保活票,节点仍由现有 `start_node` / `shutdown_node` 拥有。fork 的 config plugin 处理 FGS manifest / 权限 / `startForeground` 时序 / Android 14 类型化 / timeout。
- 备选:自写 Kotlin Service —— 否决(用户选库);保留为 fork 失维时回退。

### D6: FGS 生命周期严格绑定 `runtimeState`
`startNode` 成功后拉起 FGS、`shutdownNode` 时停 FGS,node running ⇔ FGS up;只在节点启动 / 前台切换时启,不从纯后台冷启;加幂等。避免悬挂保活通知与 `ForegroundServiceDidNotStartInTime`。

### D7: iOS 明确不做深后台(可寻址性,非仅"没模式")
不加 `UIBackgroundModes` / BGTask。进程挂起 → relay v2 预约几十秒掉线、dcutr 打洞死 → 对端寻址不到;relay v2 无 store-and-forward,BGTask 唤醒也救不了。iOS 只承诺前台 / 刚退台短窗口。

### D8: 传输进度 ongoing 通知(Android)+ iOS 仅应用内
Android:一条 ongoing 通知,`TransferProgress` 事件驱动按 id `update`(`onlyAlertOnce` 防重复响铃 + ≥500ms/百分比变化限流),含进度条 + pause/cancel action(经 fork action 事件路由回 transfer manager);它与 idle 保活通知是同一条 FGS 通知(内容随传输态切换,结束回 idle);完成 / 失败 / 取消后收敛。iOS:进度仅应用内(复用既有 transfer-store 驱动的应用内 UI),Live Activities 另立项。

**FGS 类型(3.1 已决策)**:统一用 `connectedDevice`(`FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE`),覆盖"常驻收配对保活"与"active transfer 进度"两阶段。选它而非 `dataSync` 是为规避 Android 15 对 dataSync 的 ~6h/24h 累计运行上限;P2P 传输作为"与另一设备的持续连接"语义上也贴 connectedDevice。notify-kit 库自带的 `app.notifee.core.ForegroundService` 声明未带 type,由本地 config plugin `with-android-foreground-service` 补 `foregroundServiceType=connectedDevice` + `FOREGROUND_SERVICE_CONNECTED_DEVICE` 权限。

## Risks / Trade-offs

- [`react-native-notify-kit` 单人维护(bus factor)] → 缓解:它是 Notifee 成熟代码的 drop-in 改名、被官方钦点;锁版本、盯上游;万一失维可自 vendor 或按 D1/D5 备选回退到 expo-notifications + 自写 native。
- [`expo-notifications` → notify-kit 迁移回归(权限 / 渠道 / 点击行为变化)] → 分阶段迁移,先切告警再上 FGS/进度;真机回归。
- [Android 15 `dataSync` FGS ~6h/24h 上限] → 已通过选用 `connectedDevice` 类型规避(见 D8);connectedDevice 不受同款上限。
- [国产 OEM 仍可能杀后台] → 电池优化豁免 / 自启动引导(可选)+ 目标机型真机存活率验证;不承诺 100%。
- [FGS 与 node 启停竞态 / 空 JS runner 泄漏] → 单一真源 `runtimeState` 成对启停 + 幂等;runner 在 stop 时确实 resolve/清理。
- [深 Doze 下 keep-alive 与 relay 预约稳定性未量化] → 持 partial WAKE_LOCK;外部 core relay 参数未知(见 Open Questions)。
- [进度通知高频 update 抖动 / 电量] → `onlyAlertOnce` + 限流(如 ≥500ms 或进度变化阈值才刷新)。
- [原生改动破坏 prebuild 可复现] → 全经 config plugin;CI 跑 `expo prebuild --clean` 校验。

## Migration Plan

1. 阶段一(告警迁移):`notifier.ts` 从 expo-notifications 切到 notify-kit(告警展示、深链、权限、渠道),移除 expo-notifications。需一次 `expo prebuild` + dev-client 重装。可独立验证。
2. 阶段二(FGS 保活):接 fork 的 foreground service + `mobile-core-store` 生命周期绑定 —— arm64 原生重编 + 真机(含国产 OEM)验证后台收配对。
3. 阶段三(进度通知):`transfer-store` 驱动 ongoing 进度通知 + pause/cancel action 路由。
4. 回滚:FGS/进度出问题时从 store 摘除 start/stop 与 update 调用即回到"仅告警 / 仅前台";若 fork 整体不可用,按 D1/D5 备选回退 expo-notifications + 自写 native。

## Open Questions

- ~~FGS 类型~~(已决策):统一用 `connectedDevice` 规避 Android 15 dataSync 时长上限,覆盖保活 + 传输两阶段。待 Play 审核实测 connectedDevice 对纯 P2P 是否被接受;若被拒退回 `dataSync` + 处理 6h 上限或申请 `specialUse`。
- notify-kit 维护活跃度与发版频率需持续盯;是否值得预先准备自 vendor 方案。
- 外部 core(github `swarm-apps/SwarmDrop` rev `5b39fb44`)的 relay 预约 TTL / keep-alive / dcutr 参数未量化 —— 决定 iOS"刚退台短窗口"实用价值与 Android FGS 深 Doze 下 keep-alive 是否真稳。
- 目标用户是否含激进杀后台的国产 OEM —— 决定电池豁免引导与真机存活率验证的投入。
