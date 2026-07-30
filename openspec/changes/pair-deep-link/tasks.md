# pair-deep-link 任务分解

> 依赖 `invite-url-canonical`（前缀校验与解析走 canonical base URL 与统一解析入口）。

## Phase 0 — plugin 与 scheme 注册

- [x] 装 `tauri-plugin-deep-link` 2.4.9（Rust + JS，正是 pair-invite-ui design D4 预言的版本）
- [x] `tauri.conf.json`：`plugins.deep-link.desktop.schemes = ["swarmdrop"]`
- [x] `setup.rs` 注册 plugin，注释写明**只用它的 scheme 注册能力**、事件消费仍走自建 handler
- [x] dev 下 `register_all()`（仅 Windows / Linux 且仅 debug —— 打包后由 installer / plist 完成；
      没有它 `pnpm tauri dev` 里点深链系统找不到处理程序，根本测不了）
- [ ] **待真机验证**：macOS 上 plugin 是否也挂 `RunEvent::Opened`、两个消费者会不会互相吃掉
      事件。需要跑 `pnpm tauri dev` 并点一次 `swarmdrop://` 链接。
      若发现抢事件 → 退路是不装 plugin、自己写三平台注册（design D1 末尾）

## Phase 1 — 外部入口分发器（src-tauri）

- [x] `external_open.rs` 泛化为**外部入口分发器**：模块文档改写成两类负载的表格
- [x] `dispatch_url`：`file://` → 路径，`swarmdrop://` → 邀请，其余静默忽略 + debug 日志
- [x] macOS `handle_opened` 改为逐个 URL 走 `dispatch_url`（原来是 `filter_map(to_file_path)`
      —— 非 file scheme 被静默丢弃，正好是深链要走的那条）
- [x] Windows / Linux `ingest_from_args` 按 scheme 前缀分流（深链在那两个平台是一个 argv 项，
      与被打开的文件路径混在同一串参数里）
- [x] `Inner` 加 `invite: Option<String>`：**只留最后一条**，与路径的「合并成批」刻意不同
      （同时收到两条邀请是异常，攒成数组只会让前端面对一个没有正确答案的选择）
- [x] `spawn_flush` 抽出来：一个去抖窗口内两类负载各自 emit
- [x] `take_pending` 改为**一次取走两类**（返回 `PendingExternalOpen`）——
      `frontend_ready` 是共享标记，拆成两个命令会让第二类负载丢在
      「标记已置位、前端还没订阅完」的缝里
- [x] `events.rs` 加 `ExternalPairInvite` + `collect_events!` 登记
- [x] 命令薄壳与 bindings 再生（`cargo test export_ts_bindings`）
- [x] `catch_unwind` 与全局 `OnceLock` 缓冲两样机制在新形态下保持原样

## Phase 2 — 桌面前端

- [x] `external-open-handler.tsx` 改造：**先订阅两个事件、再一次拉取**缓冲负载；
      文件 → share-store → 选设备屏；邀请 → 先跳 `/pairing/input` 再 previewInvite
      （解码失败时用户正好停在能改用粘贴的那一屏）
- [x] 首启未设设备名的闸抽成 `readyForIntent()`，两类负载共用
- [ ] **待真机验证**：三平台冷启动 / 热启动各点一次深链（6 组）

## Phase 3 — Android 深链

- [x] ~~`mobile/app.json` 加 Android intent-filter~~ —— **不需要，前提是错的**。查本地 prebuild
      产物 `android/app/src/main/AndroidManifest.xml` 实测：Expo 已从顶层 `expo.scheme` 自动
      生成 `action=VIEW scheme=swarmdrop category=[DEFAULT,BROWSABLE]`。手写 `intentFilters`
      只会多一条冗余 filter。（scheme-only 的 data 规则匹配任意 URI，含 `swarmdrop:` 这种
      opaque 形态，所以单冒号深链也命中。）
- [x] **URL 分发走 `+native-intent.tsx`，不接 `Linking.addEventListener`** —— 与原计划不同。
      expo-router 的 `redirectSystemPath` 已经是本 App 唯一的 URL 分发口（iOS Share
      Extension 那条就靠它避 404），再加 Linking 监听会造出第二个消费者与它竞争同一条 URL，
      分享 URL 也会流进去。改为：拦截 → 邀请放进 `@/core/pending-deep-link` 单槽 → 返回 `/`
      → 根布局 `DeepLinkInviteHandler` 取走。与 iOS 分享那条的形状完全一致
- [x] payload → 统一解析入口 → 确认卡 → 配对（走同一个 `previewInvite`，
      成功才进 `/pairing/found-device`，安全闸与扫码/粘贴完全相同）
- [x] 单槽「先订阅、再取一次」覆盖冷/热启动两条：冷启动负载早于 React，mount 后 take 得到；
      热启动在 mount 之后放下，只能靠订阅。反过来（先 take 再订阅）会在两者之间漏掉一条
- [ ] **人工**：`expo prebuild` + Android 冷启动 / 热启动各测一次（intent filter 已由 Expo
      生成，无需改配置，但仍要真机验证 expo-router 分发与本单槽的接力）

## Phase 4 — 剪贴板检测改造

- [x] 检测提到 `_app` 布局：新建 `ClipboardInviteBanner`（全局、非模态）
- [x] **撤掉 `/pairing/input` 里的那份局部检测** —— 两份 hook 实例会各读一次剪贴板、
      各记一份「已提示过」，同一条邀请亮两次
- [x] **解码提前到检测阶段**：直接调 `decodePairInvite`，于是提示条能写出对端设备名
      （代价是每次 focus 多一次 IPC 往返 —— focus 不是高频事件）
- [x] **自我过滤**：`preview.peerId == 本机 peerId` 时静默忽略（用户复制自己刚生成的邀请
      准备发给别人，回到应用不该被问「要不要配对」）。判据是签名覆盖范围内的结构性字段
- [x] 前缀硬编码**彻底去掉**（判据交给后端唯一解析入口，前端不再需要知道链接长什么样）
- [x] 配对流程进行中（`phase !== "idle"`）不检测，避免打断正在走的那条
- [x] 非模态形态；点击 → 跳受邀方屏 + previewInvite → 确认卡（安全闸不变）
- [x] i18n：新增串已补 en / zh-TW 译文
- [x] 移动端自我过滤已做，**但不是靠「Android 后台读剪贴板」** —— 那个前提不成立：
      Android 12+ 读剪贴板同样会弹系统提示（「SwarmDrop 已粘贴…」），并非 iOS 独有。
      桌面那套「focus 时静默读 + 解码出设备名」照搬过来，两个平台都会冒出莫名提示。
      移动端现有形态本就更贴合平台，保留不动：`hasStringAsync` 只探「有没有字符串」→
      在用户自己打开的配对 sheet 里亮一枚 chip → **点了才读**。
      落地的是任务的**意图**（同一套自我过滤），位置在 `previewInvite` 这个单一收口，
      于是扫码 / 粘贴 / chip / 深链四条入口一起覆盖：`preview.peerId == selfPeerId` 时拒绝
      并置 `rejectedAsSelf`，三处 UI 各自本地化成「这是你自己的邀请」。
      - 本机 peerId 由 `mobile-core-store` 在身份就绪后**推**进邀请 store —— 反向 import
        会成环（那边已 import 这边），而 core 的 `initializeIdentity` 是异步的、没有同步 getter
      - 文案没塞进 store 的 `error`：那里现存的是硬编码中文 + core 透传的 Rust 错误串，
        本身就是 i18n 漏洞（见下方待办），不该再往里加

## Phase 5 — 落地页联动（docs）

- [x] 「在 App 中打开」按钮 —— **深链形态是 `swarmdrop:` 单冒号，不是 `swarmdrop://`**
      （见下方「实测推翻」）。按钮默认 `hidden`，只有脚本判定非 iOS 才显示，所以无 JS 时
      也不会出现死按钮。点击后用「1.6s 后仍在前台」当作没打开的信号补一句提示 ——
      各浏览器都不告诉页面结果，这是唯一可用的判据，误判成本只是多一句话
- [x] iOS / iPadOS 隐藏该按钮（iPadOS 13+ 的 UA 伪装成 Macintosh，靠 `maxTouchPoints` 补判）
- [x] 落地页体积 5.4KB gzip（预算 <10KB）
- [ ] 「记住我的选择」（有两条路径后才有意义）

## Phase 6 — 门禁与验收

- [x] `cargo fmt --all` / `cargo check --workspace --all-targets` / `cargo test --workspace`
      （47 个测试组全绿）
- [x] `./scripts/check-wasm.sh`
- [x] `pnpm exec tsc --noEmit`、`pnpm test`（64 passed）、`pnpm check:clipboard`、
      `pnpm check:zustand-access`、`mobile` 下 `pnpm typecheck`、`docs` 下 `pnpm build`
- [ ] **待真机验证 — share-target 回归**（本 change 最需要防的）：macOS「打开方式」多选文件、
      Windows / Linux 冷启动与已运行 argv 三条路径各走一次
- [ ] **待真机验证 — 深链冒烟矩阵**：macOS / Windows / Linux / Android × 冷启动 / 热启动
- [ ] **待真机验证 — 剪贴板**：复制自己的邀请（不该亮）、复制别人的（该亮且带设备名）、
      忽略后切路由多次聚焦（不该重复亮）
- [ ] 知识库：`rust-backend.md` 的 external_open 一节改写为「外部入口分发器」，
      记 macOS 事件分流的实测结论（等 Phase 0 验证完再写，免得记一个没验证的假设）

## 未完成项的原因

- **Phase 3（Android 深链）**：`app.json` 改 intent-filter 后必须 `expo prebuild` 重编原生，
  且验证要连真机 / 模拟器。代码可以先写，但「写了没验过的原生配置」价值有限，故与 Phase 0
  的实测一起排。
- **Phase 5（落地页深链按钮）**：依赖 Phase 0 确认 scheme 真的能被唤起，否则就是加一个死按钮。
- **所有 `待真机验证` 项**：需要 GUI 会话跑 `pnpm tauri dev` / 真机点链接，无法在当前环境完成。

## 审查带出的待办（2026-07-30 三路并行审查）

- [ ] **把 `mobile/src/app/pairing/scan.tsx` 的粘贴路径改成直接问 core**（扫码那条保留正则
      —— 它需要「不匹配就静默继续扫」这个廉价判据，每帧一次 IPC 不可行）。这样最后一处
      前端硬编码前缀消失，「改域名漏改导致扫码静默失效」的风险从根上没了。
      同文件的 `invite-exchange.tsx:284` 其实已经是「整段丢给 core」的形态 —— mobile 内部
      现在有两种做法。
- [ ] **两份邀请列表不自动刷新**（`pairing-panel.tsx` / `-sent-invites-section.tsx`）：
      对方消费邀请后「等待对方使用」不会翻成「已被对方使用」，除非用户离开再回来。
      要先定刷新时机（轮询 vs 等 `PairedDeviceAdded` 事件），不宜临时塞定时器。
- [ ] **多条邀请同时出现在剪贴板时静默取第一个**（`extract_payload` 用 `find`）。
      不是可利用提权（诱饵必须是攻击者自签的合法邀请、且必经用户确认），但 UI 至少该有
      得知的能力。改动会碰 IPC 契约。
- [ ] **落地页主路径在跨 browsing context 时静默失效**：sessionStorage 是 per-tab，
      「复制链接地址 → 粘到另一个窗口」必丢，且 fragment 兜底只在 `setItem` 抛异常时启用、
      不覆盖「handoff 丢失」。这是 design D5 主动选的权衡，但失败是静默的 —— 值得实测各
      浏览器的 `target=_blank` / 右键新窗口克隆行为。
- [ ] **前端零 CI 门禁**：`.github/workflows/` 只有 `rust.yml`，`tsc` / `vitest` /
      `check:zustand-access` / `check:clipboard` 都没人跑。加前端 workflow 会改变所有 PR 的
      通过条件，是独立决策，需用户确认。
- [ ] **`windows-registry` 0.5.3 与 0.6.1 并存**（`tauri-plugin-deep-link` 引入前者）。
      `rust.yml` 只跑 ubuntu，Windows 编译问题要到打 `v*` tag 才暴露 —— 发版前在 Windows 上验一次。

## 实测推翻的一条设计（2026-07-30）

- [x] **深链形态从 `swarmdrop://` 改成 `swarmdrop:`（单冒号）。** 原先只有散文描述、零测试。
      实测 `url::Url` 的往返：

      | 形态 | 序列化后 | decode 能否定位前缀 |
      |---|---|---|
      | `swarmdrop://https://swarmapp.cn/p/#…` | `swarmdrop://https**//**swarmapp.cn/p/#…` | **否** |
      | `swarmdrop:https://swarmapp.cn/p/#…` | 原样 | 是 |

      `//` 之后 `https:` 被当成 authority（host=`https` + 空端口），WHATWG 序列化丢掉空端口
      的冒号，canonical 前缀就断了。**只在 macOS 上暴露**（那条走 `RunEvent::Opened`，拿到的
      是已解析的 `url::Url`）；Windows / Linux 走 argv 原样字符串，两种形态都能认 ——
      典型的平台分叉静默失败。
      已由 `src-tauri/src/external_open.rs` 的 `tests::deep_link_contract` 钉死（连坏形态一起
      钉：它断言 `//` 形态**确实**会坏，并写明这条红了该怎么做）。落地页与移动端的生成/解析
      都按单冒号形态对齐。

## 收尾带出的待办

- [x] **移动端 `previewInvite` / `confirmInvite` 的错误串已过 i18n**（用户「翻译可以大胆改」）。
      修法不是加翻译层，而是**把分类给对** —— 两端本来就有语言中立判别码，只是没用上：
      - 桌面 `decode_pair_invite` 原先包成 `AppError::identity(...)` → 前端按 `kind` 渲染成
        「设备身份初始化失败」，一条与「链接不对」毫无关系的提示。改成 `InvalidCode`
      - 移动 `decode_pair_invite` 原先 `FfiError::Identity(format!("邀请无效: {e}"))`
        → 直接把 Rust 中文串甩到 UI。改成 `FfiError::InvalidCode`
      - **`pairing_result` 的 `reason` 原先是 `format!("{reason:?}")`** —— UI 上显示的是
        `UserRejected` 这种 Rust 裸标识符，比翻译泄漏更糟。改成稳定 snake_case 判别码
        （穷尽 match，加变体时编译失败）。**FFI 形状不变**，所以不需要重新生成 bindings
      - store 用 `previewReject: "expired" | "self" | "invalid"` 与
        `confirmReject: "userRejected" | "failed"` 两个判别码取代硬编码文案；技术细节只进
        `console.warn`。原先 `error` 字段兼做生成态/确认态展示，现在预览失败不再污染它
      - 三处 UI（scan / invite-exchange / 深链 handler）+ 确认页各自本地化
      - 早先加的 `rejectedAsSelf` 布尔被 `previewReject` 吸收，不留两套机制
- [x] 两端 catalog 补齐：桌面 en / zh-TW、移动 en 全部 **0 条空译、0 条英文位置残留中文**
      （含此前遗留的 `就绪 · 配置节点` / `配置节点`）
- [x] 顺带修掉移动端 `INVITE_TTL_SECS = 300` 的漏改 —— 那不是显示问题：
      `scheduleRefresh` 到点重新生成，而生成前**先撤销旧邀请**，于是用户发出的链接会在
      5 分钟后被自己的 App 悄悄作废。连带把 `mm:ss` 换成按量级换单位（24h 会显示成 1439:59）
