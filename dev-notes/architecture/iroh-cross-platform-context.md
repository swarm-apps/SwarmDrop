# Iroh 迁移与跨端运行时：讨论上下文

> 用途：给后续参与 SwarmDrop 的 AI / 开发者提供架构上下文。  
> 状态：讨论结论与目标架构，不代表已经完成实现。  
> 更新：2026-07-16  
> 文档语言：中文。

## 1. 一句话结论

SwarmDrop 应从“桌面 Tauri 内嵌 libp2p 节点”的形态，演进为同一份 Rust 核心服务多个宿主：

~~~text
桌面：swarmdropd 持有唯一 Iroh Endpoint；Tauri GUI、CLI、TUI 通过本机 IPC 控制它
移动：React Native 的 UniFFI Native Module 在 App 进程内直接持有 Runtime
Web：浏览器 tab 内运行 WASM 临时 Runtime，按 relay-only 临时端设计
~~~

二维码和分享链接是同一个一次性 PairInvite；不再使用“6 位码 → DHT 记录”建立信任。

## 2. 当前事实与约束

- 主仓库是桌面端 SwarmDrop：React/Tauri 在根目录的 src 与 src-tauri。
- Rust workspace 已有 crates/core、crates/entity、crates/migration。
- crates/core 已有设备、配对、传输、网络管理及 Host Trait；桌面端是当前宿主。
- 另有 SwarmDrop-RN 仓库；其中的 packages/swarmdrop-core 是 UniFFI/mobile wrapper，固定引用主库的 Rust core revision。
- 当前网络实现仍基于 libp2p 和 libs/core 子模块；Iroh 迁移尚未开始。

### 2.1 旧配对码的问题

当前“6 位数字码 → DHT 查询候选节点”的方案不适合作为长期信任建立机制：

- 码空间小，容易枚举、碰撞、抢注；
- DHT 只能返回候选地址，不能证明远端就是用户确认的设备；
- 记录覆盖、TTL 与网络抖动会造成失败或静默误配对；
- 不适合网页、桌面、移动和 CLI 间传播。

结论：低熵码不能再作为可信身份锚点。

## 3. Iroh 的定位

Iroh 是连接层，不是 libp2p Behaviour 积木箱的逐项替代：

- 稳定设备身份是 EndpointId。
- EndpointAddr 是会变化的连接提示，可能包含 relay URL 和直连地址；它不是长期身份。
- Iroh 处理 QUIC、NAT 打洞、路径选择和 relay fallback。
- SwarmDrop 仍自行定义信任、邀请、文件授权、收件箱、业务控制协议和产品级发现规则。

迁移后不应继续以 Swarm、NetworkBehaviour、低熵 DHT 配对码为架构中心。Iroh 的产品抽象应围绕 Endpoint、连接和 QUIC stream。

## 4. 网络与中继策略

### 4.1 原生端

原生端的连接优先级：

~~~text
局域网直连 → UDP/QUIC 打洞直连 → 项目 relay 转发
~~~

不要把 UDP 直连承诺为中国网络环境下的稳定前提。企业/校园网、公共 Wi-Fi、严格 NAT、部分蜂窝和跨网链路可能限制 UDP 或使打洞不稳定。

relay 是可靠性路径，不是错误路径：

- 直连成功时降低延迟与带宽成本；
- 直连失败时仍通过端到端加密 relay 传输；
- 生产环境应评估并配置自己的 relay，不应把免费公共 relay 当作生产 SLA；
- 中国用户必须通过真实网络矩阵验证 relay 可达性、延迟和稳定性。

### 4.2 浏览器端

浏览器没有原生 UDP socket，不能实现 Iroh 原生端的 QUIC 打洞，也不能承诺严格 LocalOnly。

浏览器端定位为：

- 无需安装的临时收发端；
- relay-only；
- 默认不保存长期信任或长期文件状态；
- LocalOnly 时明确引导用户打开原生应用；
- 不把 iroh-blobs 当作浏览器大文件传输终局，先验证内存、持久化和恢复能力；必要时使用自研 transfer 的精简 Web 路径。

Web 不应驱动 Iroh 迁移排期。先完成桌面与移动锁步迁移，再做范围很窄、可关闭的 Web PoC。

## 5. 邀请链接与二维码配对

### 5.1 用户入口

二维码就是完整邀请链接的图形编码；扫码、点击、复制和粘贴必须得到同一份邀请。

推荐形式：

~~~text
https://swarmdrop.app/i#<base64url(PairInvite)>
~~~

fragment 让自包含邀请内容不随普通 HTTP 请求发送给网站服务器。各平台对深链和 fragment 的保留行为必须做真机 PoC；不可靠时可退到高熵、短 TTL、一次性的 opaque token，但绝不回退为 6 位码。

链接处理顺序：

1. Universal Link / App Link 唤起原生 App；
2. 桌面端使用自定义协议或落地页“打开应用”；
3. 无原生 App 时进入 Web 临时端。

### 5.2 PairInvite

PairInvite 至少包含：

- version；
- invite_id；
- 至少 128 bit、推荐 256 bit 的一次性 capability；
- 发起方 EndpointId；
- 发起方 EndpointAddr；
- issued_at / expires_at，默认建议 5 分钟；
- transport_policy：Auto 或 LocalOnly；
- 仅展示用的设备提示信息；
- 发起方身份对规范化内容的签名。

发起方只持久化 capability 哈希、邀请 ID、过期时间和使用状态；不得记录明文 capability。

### 5.3 配对安全流程

1. 接收方解析邀请、验签、检查版本与 TTL。
2. 接收方按邀请中的 EndpointId、EndpointAddr 建立 Iroh 连接。
3. 握手完成后验证实际远端 EndpointId 与邀请声明一致。
4. 接收方发送 invite_id、capability 和 receiver_endpoint_id。
5. 发起方校验 capability 哈希、未过期、未使用、未撤销。
6. 双方 UI 展示设备名称、平台、短指纹，并要求显式确认。
7. PairAccept 和 PairCommit 绑定邀请 ID、双方 EndpointId 与会话摘要，防止重放。
8. 成功后双方写入 PairedDevice；capability 立即消费。

长期配对记录只以 EndpointId 为身份锚点；不要固化 IP、端口或 relay URL，也不要维护公共在线设备目录。

控制协议先以自定义 ALPN + QUIC 双向 stream 为基线。是否使用 irpc / irpc-iroh 是后续 PoC 决策，不能让 RPC 框架反过来决定产品协议。

## 6. 网络诊断

Iroh 诊断的关键维度包括 UDP/IPv4/IPv6 连通性、NAT 类型、映射是否随目的地变化、UPnP/PCP/NAT-PMP、relay 延迟和 captive portal。

SwarmDrop 应提供“设置 → 网络诊断”：

- 用户摘要：直连良好 / 将优先使用中继 / 网络限制较多；
- 详情：UDP、NAT、relay 延迟、端口映射、当前路径；
- 支持：复制脱敏诊断信息；
- 隐私：不默认上传公网 IP、局域网地址、EndpointId。

第一版只做本地诊断。若后续接入 Iroh Services 的远程诊断，必须是用户明确授权的支持模式，不得默认授予远程诊断权限。

## 7. CLI、TUI 与守护进程

### 7.1 Runtime 职责

一次性命令可以完成发送后退出；但 NAS、服务器、树莓派或无 GUI 设备若要“随时可收文件”，必须有常驻 Runtime。

~~~text
swarmdropd      唯一持有 Iroh Endpoint、收件箱、传输任务和配对状态
swarmdrop       命令式 CLI，通过本机 IPC 控制 daemon
swarmdrop tui   交互式终端 UI，通过同一 IPC 控制 daemon
Tauri GUI       桌面 UI，通过同一 IPC 控制 daemon
~~~

命令示例：

~~~text
swarmdrop pair create
swarmdrop pair accept <invite-url>
swarmdrop send <file> --to <device>
swarmdrop receive --foreground
swarmdrop diagnose --json
swarmdrop tui
~~~

交互策略：

- 在 TTY 且无子命令时可进入 TUI；
- 有子命令、管道输入或 JSON 输出时保持机器可读的命令式行为；
- 无 GUI 配对也必须展示短指纹并要求确认；
- 自动接收仅允许用户显式配置的可信设备策略。

### 7.2 本机 IPC

desktop daemon 与客户端使用本机 IPC，不监听 localhost：

- macOS/Linux：Unix domain socket；
- Windows：named pipe；
- 协议为版本化 Request / Response / Event；
- 支持持久事件订阅，例如传输进度、配对请求、网络状态；
- 一个 profile 只能有一个 Endpoint，用锁和 socket 防止重复 daemon；
- 按平台验证 socket 权限、pipe ACL 与本机身份校验。

推荐实施库：Tokio + interprocess；CLI 用 clap；TUI 用 ratatui。tarpc 可作为 RPC PoC 候选，但不是必须的架构锚点。

### 7.3 Tauri command 的职责

Tauri command 不会消失，而是变薄：

~~~text
React WebView → Tauri command → daemon IPC → Runtime
~~~

保留在 Tauri 的职责：

- WebView 到 Runtime 的安全桥接；
- 文件选择、打开目录、系统通知、托盘、窗口、深链；
- Keychain 授权、登录项、自动更新等桌面专属能力。

不应继续由 Tauri command 持有网络生命周期、NetManagerState 或 Iroh Endpoint。

### 7.4 macOS

macOS 的“窗口关闭后常驻”和“无 GUI 也能持续接收”是两件事：

- 关闭窗口隐藏到托盘：桌面体验；
- 登录后持续接收：用户级 LaunchAgent 持有 swarmdropd。

应使用用户级 LaunchAgent，而不是 root LaunchDaemon：设备身份、Keychain、文件目录和收件箱均属于登录用户。正式 App 分发需评估 SMAppService、用户在“登录项”中禁用后台服务，以及 Local Network 权限。

## 8. 移动端与 Web 的 Runtime 边界

| 宿主 | Runtime 所在位置 | 是否使用 desktop IPC | 持续在线能力 |
| --- | --- | --- | --- |
| Desktop GUI / CLI / TUI | swarmdropd 独立进程 | 是 | 是，取决于用户是否启用后台服务 |
| 服务器 / NAS | swarmdropd | CLI 可通过本机 IPC | 是 |
| React Native | App 进程内 UniFFI Native Module | 否 | 前台为主；后台受 iOS/Android 生命周期限制 |
| Web | 浏览器 tab 内 WASM | 否 | 临时；关闭页面即结束 |

移动端不跑 swarmdropd。它直接嵌入同一份 Runtime，并通过 UniFFI callback 接收事件。iOS 不能承诺桌面 daemon 一样长期后台接收；移动端应做前台优先、状态恢复与平台许可下的有限后台能力。

## 9. 绑定、WASM 与 TypeScript 不漂移

目标不是把整个 native core 编译为 WASM。应保留纯业务核心，并增加很薄的 js-api facade：

~~~text
crates/core             业务规则与 Runtime 内部能力；不依赖 UniFFI
crates/js-api           UniFFI Record / Enum / Object / async API 投影
packages/bindings       ubrn 配置与生成的 RN、Web TypeScript/WASM 产物
apps/web                只 import 生成后的 bindings
apps/mobile             只 import 同一 bindings 包
~~~

推荐 UniFFI + uniffi-bindgen-react-native：

- 同一份 Rust UniFFI 导出可生成 React Native 和 Web/WASM 的 TypeScript bindings；
- Web 构建生成 wasm-bindgen crate、WASM、JS glue 和 TypeScript 声明；
- Rust async 映射为 Promise；Record/Enum/Object/callback 生成对应 TS 类型；
- 生成文件禁止手改。

wasm-bindgen 本身也能生成函数/类声明，但复杂业务数据容易退化为 JsValue。ts-rs、Specta 仅负责 TS 类型导出，不负责 WASM 运行时绑定。Web/RN 应让 UniFFI facade 成为唯一类型源；Tauri 可继续使用现有 Specta 导出 desktop command 类型。

CI 必须：

1. 重新生成 RN/Web bindings；
2. 检查生成目录是否与 Git 一致；
3. 分别运行 Web、RN 的 TypeScript typecheck；
4. 接口变更后重建 iOS/Android native artifact，避免 TS 已更新但 xcframework 或 so 过期。

## 10. 仓库组织：目标是单仓

当前 SwarmDrop-RN 已通过 Git revision 共享主库 core，属于“伪 monorepo”。Iroh 迁移、邀请协议和 bindings 同时修改时，跨仓同步成本高。

建议将移动端迁入主仓；不必现在新建仓库：

~~~text
SwarmDrop/
├── src/ + src-tauri/             桌面端先保持原位，避免迁移噪声
├── apps/
│   ├── mobile/                   导入原 SwarmDrop-RN App
│   └── web/
├── crates/
│   ├── core/
│   ├── contracts/                PairInvite 等纯跨端契约
│   ├── js-api/
│   ├── runtime/
│   ├── ipc/
│   ├── daemon/
│   ├── cli/
│   ├── wasm/
│   ├── entity/
│   └── migration/
├── packages/
│   └── swarmdrop-bindings/       原 apps/mobile/packages/swarmdrop-core 的升级形态
├── docs/
└── e2e/
~~~

边界：

- 迁入的是移动 App、绑定源码和测试；不是把移动 UI 逻辑塞进 Rust core。
- 原 packages/swarmdrop-core 应改名为 @swarmdrop/bindings 或 @swarmdrop/runtime，因为它不再只服务 mobile-core。
- apps/web 是独立部署的浏览器应用，不放进当前桌面 React 的 src。
- crates/cli 不放进 src-tauri，CLI 不依赖 Tauri。
- 未来若跨端 API 已稳定且确有独立发布需求，可再抽出独立 core 仓；当前不要为了目录好看过早拆仓。

迁移步骤：

1. 使用 git subtree 将 SwarmDrop-RN 历史导入 apps/mobile。
2. 第一阶段只修 workspace、相对路径和 CI，不改业务行为。
3. 将 UniFFI/WASM bindings 提升到根目录 packages/swarmdrop-bindings。
4. 验证桌面、iOS、Android 的既有构建后，再开始 Iroh 锁步迁移。
5. 旧移动仓归档，并在 README 指向主仓。

## 11. 实施顺序

建议顺序，不代表已排期：

1. 完成单仓迁移和共享 contracts/bindings 边界；
2. 做 Iroh 网络与跨端编译 spike：桌面、iOS、Android、WASM 分别验证；
3. 桌面与移动锁步替换 libp2p 身份、连接与配对；旧 PeerId 不做静默信任迁移，要求显式重新配对；
4. 落地邀请链接/二维码配对，删除低熵 DHT 配对码；
5. 增加 desktop daemon、CLI/TUI 和本机 IPC；
6. 最后做范围很窄、可关闭的 Web 临时端 PoC。

libp2p 与 Iroh 的 wire/身份体系不兼容，不要假设可无感混用。

## 12. 仍需 PoC / 未决项

- Iroh 在 iOS、Android、WASM 上的实际编译、体积、内存和网络可达性；
- 中国三大运营商、企业网、校园网、公共 Wi-Fi 的 UDP 直连率和 relay 体验；
- 自建 relay 的地域、成本、可达性、合规和监控；
- Web deep link 是否可靠保留 fragment；
- 浏览器大文件传输是否使用自研精简 transfer，而非 iroh-blobs；
- irpc-iroh 与手写 QUIC control stream 的取舍；
- macOS LaunchAgent / SMAppService 的打包与权限路径；
- IPC 协议版本、事件背压、socket/pipe 安全与多 profile；
- Iroh EndpointId 与旧 PeerId 配对记录的迁移 UX。

## 13. 相关文档与资料

内部：

- [邀请链接与二维码配对设计](../iroh-invite-link-pairing-design.md)
- [Iroh + Web + CLI 开工路线报告](../iroh-web-cli-recon-2026-07.md)
- [Rendezvous 与配对风险调研](../rendezvous-recon-2026-07.md)
- [Core / Desktop / Mobile 架构边界](core-desktop-mobile-boundaries.md)
- [未来 OpenSpec 候选项](future-openspec-candidates.md)

外部：

- [Iroh Relays](https://docs.iroh.computer/concepts/relays)
- [Iroh Network Diagnostics](https://docs.iroh.computer/iroh-services/net-diagnostics/usage)
- [Iroh WASM / Browser](https://docs.iroh.computer/languages/wasm-browser)
- [UniFFI JavaScript / React Native bindings](https://github.com/jhugman/uniffi-bindgen-react-native)
- [Tauri Sidecar](https://v2.tauri.app/develop/sidecar/)
- [Apple SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)
