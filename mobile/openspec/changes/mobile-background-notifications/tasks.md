## 1. 通知栈迁移到 react-native-notify-kit

- [ ] 1.1 安装 `react-native-notify-kit`(已装 v10.4.6,typecheck 通过)+ config plugin(用本地 `with-android-foreground-service`,已挂 app.json);`expo prebuild` + dev-client 重装跑通 **待本机执行**
- [x] 1.2 `notifier.ts` 从 `expo-notifications` 迁到 fork:配对 / 传输 offer 告警展示(前台直接展示,无 handler 抑制)
- [x] 1.3 从依赖与 `app.json` 移除 `expo-notifications`,确认无两套通知栈残留
- [ ] 1.4 验证前台收到 `PairingRequestReceived` / `TransferOfferReceived` 时系统通知弹出,且 in-app 弹窗不回归、两通道对同一 pending 请求幂等

## 2. 告警深链、权限与渠道

- [x] 2.1 告警 `content.data` 带路由信息(`kind` + `pendingId` / `sessionId`)
- [x] 2.2 用 fork 的前台 / 后台事件 + 初始通知事件处理点击,经 `swarmdrop` scheme 导航到配对 / 传输界面(含冷启动)
- [x] 2.3 权限申请三层:onboarding 完成「进入 SwarmDrop」时申请(主,有上下文,优于冷启动裸弹)+ 节点启动兜底 + 设置页「通知」段深链 `openNotificationSettings`;`notifier.ts` 惰性申请降为最末兜底
- [x] 2.4 建专用高优先级 Android 渠道(`ensureAlertChannel`,告警显式指定 channelId)。注:单色图标 / tint 的 config plugin 配置留原生阶段(需图标资源)
- [ ] 2.5 验证 Android 告警品牌图标 / heads-up 抬头;运行中与冷启动点击都落到正确界面

## 3. 选型收敛(实现前拍板 Open Questions)

- [x] 3.1 FGS 类型已定:统一用 `connectedDevice`(规避 Android 15 dataSync ~6h/24h 上限),覆盖保活 + 传输两阶段
- [ ] 3.2 确认目标机型是否含激进杀后台国产 OEM,定电池豁免 / 自启动引导投入
- [ ] 3.3 记录 / 询问外部 core 的 relay 预约 TTL、keep-alive、dcutr 参数,评估深 Doze 保活稳定性
- [ ] 3.4 评估 notify-kit 维护活跃度,决定是否预备自 vendor / 锁版本策略

## 4. Android 前台服务(保活)

- [x] 4.1 `foreground-service.ts`:`registerForegroundService`(pending 不 resolve 的 runner)+ `asForegroundService` 通知实现同进程保活;`stopForegroundService` 拆除
- [x] 4.2 服务运行在主进程(config plugin 未设 `android:process`,notify-kit 库声明也无 `:remote`),共享现有 `net_manager` / tokio
- [x] 4.3 config plugin `with-android-foreground-service`:补 `FOREGROUND_SERVICE_CONNECTED_DEVICE` 权限 + 给 notifee ForegroundService 补 `foregroundServiceType=connectedDevice`(已挂 app.json,`expo config` 加载通过 + 变换逻辑单测通过);`expo prebuild --clean` 真机校验留 7.4
- [x] 4.4 `mobile-core-store`:`startNode` running 后 `startForegroundKeepAlive`、`shutdownNode` 时 `stopForegroundKeepAlive`(node running ⇔ FGS up),幂等
- [ ] 4.5 验证停止节点后常驻通知被移除、无悬挂保活通知(真机)

## 5. 传输进度常驻通知(Android)

- [x] 5.1 `event-bus` 的 `TransferProgress` → `updateTransferProgress` 按 id `update` 进度通知(进度条 + 文件数 + 速率),`onlyAlertOnce` + ≥500ms/百分比变化限流
- [x] 5.2 进度通知设为 ongoing(传输中不可划掉),active transfer 由 FGS(`connectedDevice`)承载
- [x] 5.3 pause / cancel action,经 fork 前后台事件路由到 `pauseTransfer` / `cancelTransfer`,效果与 app 内一致
- [x] 5.4 传输完成 / 失败 / 取消后 `clearTransferProgress` 回 idle 保活文案,不遗留停滞进度条

## 6. iOS 进度(仅应用内)

- [x] 6.1 iOS 传输进度复用既有应用内 UI(transfer-store 驱动,跨平台);foreground-service 全 Android 守卫,明确不实现 Live Activities
- [ ] 6.2 验证 iOS 前台 / 刚退台短窗口告警能弹;深后台被挂起时不崩溃、无误导性"已投递"状态

## 7. 验证

- [ ] 7.1 真机(同 WiFi 或跨网):Android 节点运行时退后台 / 息屏,对端发配对请求能收到并弹通知
- [ ] 7.2 真机长传退后台不被掐断,进度通知持续更新;从通知 pause/cancel 生效
- [ ] 7.3 至少一台国产 OEM 真机做后台存活率验证,记录是否需电池豁免引导
- [ ] 7.4 CI 跑 `expo prebuild --clean` 校验 config plugin 可复现;`typecheck` + `lint` 通过
- [ ] 7.5 补 / 更新 Maestro flow 覆盖通知权限与点击跳转(若可自动化)
