# pair-deep-link 设计

兑现 `pair-invite-ui/design.md` D4 留下的那笔账：
「深链（自定义 scheme）作后续独立 change —— 它需要 `tauri-plugin-deep-link@2.4.9`
+ macOS 上深链与 share-target 都钩 `RunEvent::Opened` 的分流 PoC（`external_open.rs`），
风险隔离。」

依赖 `invite-url-canonical`（前缀校验与解析走 canonical base URL 与统一解析入口）。

## D1：plugin 只用注册能力，macOS 事件消费仍自己接

装 `tauri-plugin-deep-link`，但**不让它接管事件**。

装的理由：scheme 的注册动作三平台各不相同（Windows 写注册表、Linux 生成 `.desktop`、
macOS 写 `CFBundleURLTypes` 到 plist），手写容易漏且难验证，这部分交给 plugin。

不让它接管事件的理由：**macOS 上它也是钩 `RunEvent::Opened`**，而本仓已经有一份自己的
handler（`lib.rs:42`），后者带着两样不能丢的东西：

- `catch_unwind` —— `Opened` 在 ObjC `extern "C"` 回调里触发，panic 不能跨该边界 unwind，
  否则直接 abort。这是踩过真实崩溃才加的（`external_open.rs` 头部注释有记录）。
- 全局 `OnceLock` 缓冲 —— 冷启动时 `Opened` 可能早于 `setup()` 的 `app.manage(...)`，
  那时访问托管状态会 panic。所以缓冲刻意不用 Tauri state。

两个消费者同时存在时谁先拿到、会不会互相吃掉事件，**必须实测**（tasks Phase 0 的第一件事）。
若 plugin 无法只做注册不做消费，退路是不装 plugin、自己写三平台注册（Windows 注册表项、
Linux `.desktop` 的 `MimeType=x-scheme-handler/swarmdrop`、macOS plist 经
`tauri.conf.json` 的 bundle 配置）。

## D2：`external_open.rs` 泛化成外部入口分发器，而不是复制一份

深链遇到的问题和「打开方式」一模一样：

| 问题 | share-target 现有对策 | 深链是否同样需要 |
|---|---|---|
| 冷启动时事件早于前端 mount | 全局 `OnceLock` 缓冲 + `take_pending` | 需要（点链接拉起 App 是典型冷启动） |
| 一次操作触发多个事件 | 200ms 去抖合并 | 需要程度低，但无害 |
| ObjC 边界 panic 不可 unwind | `catch_unwind` 兜底 | 需要（同一个回调） |
| Windows/Linux 已运行时再次打开 | single-instance argv 回调 | 需要（同一条路） |

所以是**同一套机制服务两种入口**，不是两套并行。改造形态：

```
                        ┌─ file://        → ExternalFileOpen  （share-target，行为不变）
RunEvent::Opened(urls) ─┤
argv / single-instance  └─ swarmdrop://   → ExternalPairInvite（新）
```

分流点放在模块内部，`lib.rs` / `setup.rs` 的调用方仍然无 `cfg`、无分支（保持
`external_open.rs` 头部注释承诺的「平台策略一律封装在本模块内」）。

**当前实现的一个现成缺口正好在这里**：`handle_opened` 现在是
`urls.iter().filter_map(|u| u.to_file_path().ok())` —— 非 `file://` 的 URL 被静默丢弃。
也就是说 macOS 侧一半的接线其实已经在了，只差把那个 `filter_map` 换成分流。

**最需要防的回归是 share-target**。文件打开的行为在改造后必须逐字节等价 ——
`file://` 分支的路径归一化、去抖、缓冲、事件负载都不许变。

## D3：Android only，iOS 深链本期不做（用户确认）

Android：`app.json` 加 intent-filter（custom scheme `swarmdrop`），RN 侧
`Linking.getInitialURL()`（冷启动）+ `Linking.addEventListener('url')`（运行时）。

iOS 不做的连带影响，明说：

- iOS 用户在浏览器点「在 App 中打开」不会有反应 —— 落地页需要按平台隐藏或降级这个按钮
  （iOS 上只显示「在浏览器中配对」+ 提示可复制链接后在 App 里粘贴）
- **iOS 剪贴板也无法自我过滤**（D4）：iOS 的剪贴板策略是 `hasStringAsync()` 只探有无、不读
  内容（不弹系统横幅），拿不到内容就 decode 不了，也就判不出是不是自己的邀请。
  iOS 保持现有「亮 chip → 点击才真读」的交互，读到自己的邀请时静默收起 chip。

## D4：剪贴板自我过滤 = `inviter_id == 本机 NodeId`

用户要求「自己复制给别人的不该弹」。判据不需要「记住我复制过什么」这类状态 ——
它天然在数据里：

```
读剪贴板 → 前缀校验（cheap，比对 canonical base URL）
              ↓
         统一解析入口 decode + 验签
              ↓
      inviter_id == 本机 NodeId ?
         ↓ 是            ↓ 否
      静默忽略          亮非模态条
```

`inviter_id` 是邀请 wire 的字段且在签名覆盖范围内（`invite.rs` 的「验签公钥从 `inviter_id`
就地恢复」），所以这是**结构性判据，不是启发式**，也无法被伪造绕过。

正向连带：decode 被提前到亮条**之前**，提示条因此能直接显示对端设备名与平台
（「张三的 MacBook 想和你配对」），而不是现在的「检测到配对邀请」。用户判断成本更低。

边界情况：用户转发别人给的邀请（A → B → C，B 复制时 inviter 是 A 不是 B）仍会亮条。
这是罕见场景，且 B 本人确实也可能想配对，接受。

## D5：呈现保持非模态（承接 pair-invite-protocol design D7）

用户最初想要模态弹窗，讨论后选定非模态 + 自我过滤。理由：

深链一旦跑通，剪贴板检测就退化成**兜底路径**（链接在终端 / 纯文本里不可点、scheme 注册失效、
或用户习惯性先复制）。给兜底路径配最强的打断，收益与代价不匹配。而 D4 的自我过滤已经消掉了
最主要的噪音来源（自己复制自己的）。

保留 D7 原有的安全闸：感知只亮入口，**用户点击才发起** —— 邀请是信任凭证，不全自动配对。
模态确认卡在点击之后出现，那一层不变。

范围与状态两处改造：

- 检测提到 `_app` 布局（全局），不再只在 `/pairing/input`
- 去重状态从组件 state 挪进 `pairing-store` —— 现在换个路由 `seen` 就丢，同一条邀请会反复亮

## D6：App Links 不做，custom scheme 保底就够

Android App Links（点 https 链接直接进 App，无选择器）需要
`https://swarmapp.cn/.well-known/assetlinks.json` 可被设备访问。而域名当前 CNAME 到 GitHub
Pages、未备案（`invite-url-canonical` design D7），国内设备访问可达性不确定 ——
验证失败时 App Links 静默不生效。

所以本期只做 custom scheme。三条路径的可靠性梯度：

```
理想   点 https 链接 → App Links 验证过 → 直接进 App     依赖域名可达 + 备案     本期不做
主力   点 https 链接 → 落地页 → 点按钮 → swarmdrop://    依赖落地页能打开       本期做
保底   复制链接 → 进 App → 剪贴板检测 → 一键条           完全不碰网络           本期做
```

custom scheme 不需要任何域名验证，装了就能跳。所以 App Links 缺失只是少了「无缝那一跳」，
不是流程断掉。等备案或境内镜像到位后再补 App Links，是一条独立的增量。

**fragment 的移交必须显式**：落地页跳 `swarmdrop://` 时要用 JS 从 `location.hash` 读出
payload 拼进 scheme URL —— 部分 Android intent 转发与 IM 内置浏览器会吃掉 `#` 之后的内容
（`invite-url-canonical` design D4）。
