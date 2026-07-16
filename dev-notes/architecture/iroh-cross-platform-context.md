# Iroh 迁移与跨端运行时：讨论上下文

> 用途：给后续参与 SwarmDrop 的 AI / 开发者提供架构上下文。  
> 状态：讨论结论与目标架构，不代表已经完成实现。  
> 更新：2026-07-16（当日经多 agent 评审校正，见下节）  
> 文档语言：中文。

## 0. ⚠️ 评审校正（必读，先于全文）

本文原稿是与 Codex 的讨论结论。同日经一轮多 agent 评审（对照三份深度调研 +
本机代码亲验）后就地校正。**读全文前先读本节**——原稿用同一种确定语气写了三类
不同置信度的内容，本节把它们分开。

### 0.1 三类内容，置信度不同

| 类别 | 属于哪些节 | 怎么用 |
|---|---|---|
| **亲验事实** | §2 仓库布局、§3 iroh 术语与身份模型 | 可直接依赖 |
| **合理设计决策** | §5.2 capability 与身份正交、§5.3 防重放、§7.4 用户级 LaunchAgent、§8 移动端不跑 daemon | 判断正确，可执行 |
| **未验证赌注** | §9 ubrn 喂两端、§7.3 桌面 daemon 化、§10 subtree 单仓 | **必须先 spike 才能承重** |

### 0.2 必须改的（按严重度）

1. **§11 的顺序是反的（critical）**：把 go/no-go 的 iroh 跨端 spike 排在了不可退的
   单仓迁移之后。这是文档内部矛盾——§12 白纸黑字承认 iroh 在 iOS/Android/WASM 的
   编译体积内存可达性未验证，§11 却按已知排序。**spike 与合仓零技术依赖**，spike 可在
   任一仓临时分支做。先合仓几周再发现 iroh 在 iOS 体积不达标或中国不可达 = 全部打水漂。
   → **spike 提到 step 1；单仓降为 spike 通过后的可延后 plumbing。**

2. **§2.1 没提三个今天正敞着、独立于 iroh 的 P0（critical）**：一份论证"配对不安全所以要
   重构"的文档不提现网正在被利用的洞，会让读者把 P0 挂在几个月后的迁移背后。见 §2.2（新增）。

3. **§2.1 的根因诊断是错的（high）**：见 §2.1 的就地修订。结论（删 DHT-码）仍然成立，
   但理由必须换——错误的诊断会误导后续 agent 去"加长码 / 加 argon2"，那两条都无效。

4. **§9 的"薄 facade 喂两端"是伪解（high）**：见 §9 的就地修订。

### 0.3 W1 就该做的（不等迁移，与本文所有决策解耦）

三个 P0 的止血**用最便宜的手段，不要预设配对终局**——因为一旦确定走 invite-link 终局，
在 libp2p 上落地 SPAKE2 那 3-4 人周只有在最终保留短码入口时才不浪费。

- [x] **删 `PairingMethod::Direct` 的零校验旁路**（已完成，commit `e8c3ba6`）。
      实际做法不是删功能而是收紧授权依据：新增 `DeviceManager::is_lan_discovered()`，
      判据是 mDNS 实际观测到的私有地址（`IdentifyReceived` 只取 `agent_version`、
      忽略对端自报的 `listen_addrs`，故不可远程伪造）；校验前置到 publish 之前；
      `handle_pairing_request` 改穷尽 match 消除 fall-through。
- [ ] `OnlineRecord` 删 `os_info`（0.5 天，DHT key 一字节不改，零兼容问题）
- [ ] `libs/bootstrap` 的 `MemoryStore` max_records 1024 → 65536（约 1000 台在线就自然满）

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

### 2.1 旧配对码的问题（已按评审重写根因）

> **原稿把根因写成「码空间小、容易枚举」并下结论「低熵码不能再作为可信身份锚点」。
> 这个诊断是错的，已重写。** 结论（删 DHT-码）不变，但理由必须准确——错误的诊断会
> 误导后续 agent 去做两件无效的事：加长码（HackerOne #1060541 逐字："Increasing OTP
> length does not fix the vulnerability"）和加 argon2（伤己不伤敌：10^6 × 1s ≈ $5.56
> 一次性买断，而合法用户每次配对多等 1-5 秒）。

**真正的根因不是熵，是架构**：定位标识与认证秘密被焊死成了同一个东西。

```rust
// crates/core/src/dht_key.rs:14 —— 明文码即公共可写 DHT 的查找键
pub fn share_code_key(code: &str) -> RecordKey {
    dht_key(NS_SHARE_CODE, code.as_bytes())   // SHA256(NS‖code)，无盐、无密钥
}
// crates/core/src/pairing/code.rs:8
const CHARSET: &[u8] = b"0123456789";   const CODE_LENGTH: usize = 6;
```

对照证据：magic-wormhole 的码只有约 **16 bit**（比本项目的 19.93 bit **还少**）却安全了
十年。差别 100% 在架构——它的 nameplate（定位，服务器分配）与 password（认证秘密，只进
SPAKE2）是正交的。同构的还有 Matter（discriminator 12 bit 公开广播 vs setup passcode
27 bit 只进 SPAKE2+）。**SwarmDrop 是这几个系统里唯一把两者合一的。**

所以被否定的是**「码 = 公共可写 DHT 的查找键」这个具体实现**，不是「短码」这个品类。
真实反例：**git-annex（一个 iroh 消费方）正是用 magic-wormhole 短码
（`11-incredible-tumeric`）在 iroh 之上做配对**——短码不需要 Kademlia，它需要的是一个
小 mailbox。

剩下三条原有诊断仍然成立：

- DHT 只能返回候选地址，不能证明远端就是用户确认的设备；
- 记录覆盖、TTL 与网络抖动会造成失败或静默误配对；
- 不适合网页、桌面、移动和 CLI 间传播。

**结论（措辞已按评审收紧）**：删掉 DHT-码。口述短码不是被判死，而是
**explicitly-deferred 的已知缺口**——现在不做是排期决策（理由见 §5.0），
不是"短码这个品类不安全"的品类判决。这两者必须区分开。

### 2.2 三个今天正敞着的 P0（新增，独立于 iroh 迁移）

原稿完全没提这些。它们是**现网 v0.7.8 上可利用的洞**，不能挂在几个月后的迁移背后：

| P0 | 状态 |
|---|---|
| **`PairingMethod::Direct` 零校验旁路**——攻击者不需要码即可冒充任意设备完成配对（爬 Kademlia 路由表拿 PeerId → dial → 发 `Pairing{Direct, os_info:{hostname:"某设备"}}` → 对方正等着那台设备就会点接受）。成本约一台 VPS + 30 行。 | ✅ 已修（`e8c3ba6`） |
| **`OnlineRecord` 全网用户名册**——无签名裸发 hostname + 真实公网 IP 到 DHT，key = `SHA256(NS‖公开的 peer_id)` 任何人可算可查，每 150 秒刷新。**换 iroh 不解决**（pkarr 同样是"知道 EndpointId 就查得到地址"），唯一的解是记录内容加密给已配对设备。 | ⬜ 待修 |
| **码碰撞**——10^6 空间，1000 个并发码 → 约 39% 碰撞率。后果不是配对失败，是 Bob1 把文件发给陌生人 Alice2（`active_code` 校验的语义是"码相等"，不是"对面是我约定的那个人"）。**不需要攻击者，随用户增长必然发生。** | ⬜ 随配对终局解决 |

另有容量炸弹：`libs/bootstrap` 的 `MemoryStore::new()` 默认 `max_records = 1024`，被活跃
配对码和每台在线设备每 150 秒重发的 `OnlineRecord` 共享——**约 1000 台设备在线就自然满，
新码从此发布失败**，同样不需要攻击者。

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

> **📌 评审补充：三条决定性反转（原稿只泛写"评估配置自己的 relay"）**
>
> 1. **iroh 的 relay fallback 就是它的 TCP fallback。** relay 协议自 0.91 起只剩
>    WebSocket，官方明确 *"iroh will work behind firewalls that only allow TCP outbound"*。
>    所以"iroh 全 QUIC 所以在中国更脆弱"是**反的**——中国移动 2025-05-17 起的 UDP 限速
>    只会把流量压到 relay，不会断线。（但注意：**iroh 没有 TCP 直连**，direct 只能走 UDP/QUIC，
>    这对 libp2p 是真 regression。）
> 2. **大陆自建 relay 可以零备案。** `47.115.172.218` 跑 iroh-relay **省略 `[tls]` 段 →
>    纯 HTTP**（源码 `iroh-relay/src/main.rs` 明写 "TLS is disabled if not present"；
>    **官方文档说的 "public IP and DNS name" 与源码矛盾，照文档执行会误杀整个迁移**）。
>    rustls 遵守 RFC 6066，**对 IP 字面量不发 SNI** → 阿里云的未备案 SNI 拦截和 GFW 的
>    SNI 封锁**都没有匹配目标**。这是大陆自建 Tailscale DERP 多年成熟打法的正规版
>    （iroh relay 官方承认自己就是 *"a revised version of the DERP protocol written by
>    Tailscale"*）。更稳的是自签 CA + 客户端 pin（`ClientBuilder::tls_client_config()` 是
>    官方一等公民能力，且 **QAD/UDP 7824 需要 TLS**，纯 HTTP 会丢 QAD 压低打洞率）。
> 3. **discovery 零域名是必做项不是可选。** 同机自建 iroh-dns-server，
>    `PkarrPublisher::builder("http://47.115.172.218:<port>")` 走 HTTP。不做的话大陆用户的
>    发现链路挂在境外 `dns.iroh.link` 上——**这是 libp2p Kademlia 方案里不存在的新增中心化
>    依赖**，别漏。
>
> **双 relay 是配置项不是代码**：RelayMap 放 `[大陆裸IP, aps1-1.relay.n0.iroh.link]`，
> iroh 启动 ping 全部、按 RTT 选 home relay、自动 failover。"大陆 vs 新加坡"两难不成立。
>
> **⚠️ 必须埋点的系统性风险**：iroh 原生 QUIC **硬编码 SNI `<base32-endpoint-id>.iroh.invalid`**
> （`iroh/src/tls/name.rs`，`pub(crate)`、受 1.0 wire 兼容保护，**应用层改不了、只能 fork 两端**）。
> GFW 自 2024-04-07 起解密 QUIC Initial 做 SNI 封锁 → **一条 `*.iroh.invalid` 规则可让全中国
> 所有 iroh 直连同时失效**。反直觉推论：**在中国，relay-over-WSS 反而是最隐蔽的通道**，
> "真 P2P 直连"才是最脆弱的那个。早期就要把强制 relay 回退做成用户可见开关。

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

### 5.0 为什么删短码：理由已按评审替换（新增）

原稿的理由（§2.1 的"码空间小"）是错的。**结论仍然成立，而正确的理由更强**——四条，
缺一条都不足以支撑删码：

1. **配对是一次性、超低频动作。** 一个用户一生的配对次数 = 设备数 × 很小的常数。
   SwarmDrop 是持久配对 + 被动接收模型，配一次之后永久 P2P。
   **对比**：magic-wormhole 的口述短码之所以是它的全部价值，是因为它是**一次性传输**
   不是持久配对——每传一次文件就要一个新码，码的 UX 就是产品的 UX。SwarmDrop 不是。
   为一个一生用 0-2 次的入口预建一台永久运维的 mailbox，是拿核心成本换边缘 UX。
2. **"无摄像头电脑"是伪真空——它预设了手机必须是发起方。** 配对可以对称：
   **电脑当发起方、屏幕显示 QR、手机扫**。电脑不需要摄像头，手机一定有。
   剩余真空缩小到"两台设备不在同一物理空间"。（这条对 §5.2 有硬性要求，见下。）
3. **SwarmDrop 的价值主张是"提供大带宽信道"，不是"提供第一条信道"。** 用户配对时
   几乎必然已有一条低带宽文本信道（微信文件传输助手在中国是国民级跨设备剪贴板）。
   "用微信传一条 200 字符的邀请，然后用 SwarmDrop 传 10GB 视频"是合理分工。
   **反直觉推论：中国恰恰是删码最安全的市场**，因为微信渗透率让"已有文本信道"这个
   前提在这里比任何地方都成立。
4. **异步性上两者等价。** "短码支持异步配对"只在 invite 的 TTL 是 5 分钟且内嵌 direct IP
   时才是短码的优势。按 §5.2 的隐私修法把 EndpointAddr 剥成 relay-only 后（relay URL 稳定），
   **TTL 可放宽到小时级，异步配对完全成立**。两条路都要求发起方在接收方打开时在线
   （PAKE 也是交互式协议）。

**真实剩余缺口（要诚实写下，不要假装没有）**：口述短码的场景收缩到"电话教远方长辈配设备"
——而那个场景下用户连 App 都不一定会装。这是 explicitly-deferred，等数据再定。

### 5.1 用户入口（顺序已按评审调整）

二维码就是完整邀请链接的图形编码；扫码、点击、复制和粘贴必须得到同一份邀请。

> **原稿以 `https://swarmdrop.app/i#…` 为主形式、且顺序是「先 Universal Link 再自定义
> 协议」。这个顺序在中国最大的分享渠道里双杀**：微信 WebView 既不支持 Universal Link
> （点击后会把该 App 的 UL 静默降级），落地页本身又是新 SPOF + 备案问题。
> 另：`swarmdrop.app` **实测无 DNS 解析**，别让一个不存在的域名继续当设计前提。

**主路径（不依赖任何域名）**：

~~~text
swarmdrop://i#<base64url(PairInvite)>     ← 自定义协议 + 纯文本 QR
~~~

**渐进增强（可选，挂了不影响已装用户）**：

~~~text
https://swarmdrop.app/i#<base64url(PairInvite)>
~~~

fragment 让自包含邀请内容不随普通 HTTP 请求发送给网站服务器（这一点原稿是对的，保留）。
各平台对深链和 fragment 的保留行为必须做真机 PoC；不可靠时可退到高熵、短 TTL、一次性的
opaque token。

链接处理顺序（**已调整**）：

1. 自定义协议 `swarmdrop://` 唤起原生 App（主路径，零域名依赖）；
2. Universal Link / App Link（**已知在微信 WebView 内不可靠，不能作为主路径**）；
3. 落地页降级为渐进增强；无原生 App 时进入 Web 临时端。

### 5.2 PairInvite

PairInvite 至少包含：

- version；
- invite_id；
- 至少 128 bit、推荐 256 bit 的一次性 capability；
- **一次性（ephemeral）EndpointId**——见下方隐私修订，**不是长期身份**；
- 发起方 EndpointAddr（**relay-only，见下**）；
- issued_at / expires_at（**relay-only 后可放宽到小时级**，见 §5.0 第 4 条）；
- transport_policy：Auto 或 LocalOnly；
- 仅展示用的设备提示信息；
- 发起方**一次性身份**对规范化内容的签名。

发起方只持久化 capability 哈希、邀请 ID、过期时间和使用状态；不得记录明文 capability。

> **⚠️ 隐私修订（评审 high）**：原稿在邀请里放的是**发起方长期 EndpointId**（= 长期
> ed25519 公钥）+ EndpointAddr（iroh 官方 ticket 文档明说会嵌入当前可达 IP）+ 长期身份的
> 签名（照泄公钥）。**这条邀请是要发去微信的**——等于把跨会话恒定的长期身份 + 真实 IP
> 永久留在腾讯的日志和聊天记录里。今天那个 6 位码 300 秒后就是一串废数字，所以
> **"与今天等价"是错的，是单向倒退**。
>
> **修法**：invite 里放**一次性 EndpointId**（iroh 允许每次配对生成临时 `SecretKey`），
> EndpointAddr 剥成 **relay-only**（不嵌 direct IP）。**真实长期身份在 PAKE/capability
> 校验通过后的加密信道内交换。** 这一刀同时买到三样：不泄长期身份、不泄真实 IP、
> 以及因为 relay URL 稳定而可以把 TTL 放宽到小时级（§5.0 第 4 条的前提）。

> **⚠️ 对称性要求（评审 high，原稿缺失）**：**任一端都必须能生成 PairInvite。**
> §5.0 第 2 条的整个论证（"无摄像头电脑是伪真空"）依赖这一条：电脑当发起方显示 QR、
> 手机扫。如果只让手机当发起方，那个真空就真实存在，删码的理由塌一条。

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

> **📌 评审补充：daemon 化是对的终态，但别和 iroh 迁移挤在同一个冲刺里。**
>
> 「现在就让 React→Tauri command→daemon IPC→Runtime、Tauri 不再持有 Endpoint」意味着
> **把一个已跑通的桌面 GUI 抽空 Endpoint 塞进 daemon**。daemon 持有 Endpoint 的唯一硬理由
> 是「无 GUI 也能持续接收」+「一台机器只能有一个 Endpoint 属主 + 一个 SQLite writer」——
> 这两条都只在 **CLI/TUI 真的落地时**才兑现（§11 step 6）。
>
> 所以：**daemon 化跟着 CLI 一起做，不要提前**。在那之前 Tauri 继续持有 Endpoint 没有坏处。
> 原料是够成熟的（interprocess 2.4.2 活跃、tokio 一等公民、SMAppService 有 Tauri 先例 +
> Rust wrapper），但 §7.2 那句「按平台验证 socket 权限、pipe ACL 与本机身份校验」
> **是一行 bullet、实为两平台各一块 winapi/unsafe 手术**（Windows 要手搓
> `SECURITY_DESCRIPTOR`/ACE），别按一行的量估。

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

> ## ⚠️ 本节经评审重写：原稿的架构框架是错的
>
> **原稿的逻辑断裂**：开篇写「目标不是把整个 native core 编译为 WASM」——**正确**；
> 紧接着画的架构图是 `crates/js-api` 投影 `crates/core`、`apps/web` import 生成的
> bindings——**而 js-api 的传递依赖就是全量 crates/core，所以这个架构恰恰就是
> 「把整个 native core 编译为 WASM」**。文档写下了原则，然后违反了它。
>
> **根源：「薄」这个字在原稿里兼职了两件不同轴的事。**
>
> | 「薄」的两种含义 | 裁决 |
> |---|---|
> | **thin surface**（表面窄、类型不漂移） | ✅ **真解，保留**。原稿关于「wasm-bindgen 直接用复杂业务数据易退化 JsValue，应让 UniFFI facade 成为唯一类型源，Tauri 继续用 Specta」的判断是对的，对 RN 共享类型有真实价值。 |
> | **thin dependency**（隔离 tokio / !Send） | ❌ **伪解**。编译墙取决于 facade **依赖什么**，不取决于 facade 自己有多少行。 |
>
> **决定性对照**：matrix-sdk-ffi 编译到 wasm 的 PR #5127，所需改动是
> *"Relaxing Send conditionally on the async_trait annotations"*——**在 SDK core 上，
> 不是 facade 上**；配套 PR #5082 标题就是 *"Comprehensive Send+Sync bound improvements
> for Wasm compatibility"*，跨 30+ trait 的横切改动。**薄 facade 能隔离的话，matrix 就
> 不用这么干。**
>
> **本项目会撞同样的墙（已本机逐行核实）**：`crates/core/src/host.rs` 的 6 个 trait
> （`KeychainProvider:43` / `EventBus:123` / `FileAccess:214` / `Notifier:263` /
> `UpdateInstaller:282` + `AppPaths:139`）**全部是无条件 `Send + Sync` + `#[async_trait]`**。
> 而 ubrn 自己的文档（`idioms/threading.md`）逐字说明这正是你必须动的地方：
> *"you should re-write the `#[async_trait::async_trait]` to not be Send for wasm32 targets"*。
> 标准写法 `#[cfg_attr(target_arch="wasm32", async_trait(?Send))]` **要乘以所有 async trait、
> 所有 impl、以及每一个持有 `Arc<dyn ...>` 的 struct 和 future 的 Send 界**。
> `wasm-unstable-single-threaded` 只放松 UniFFI **自己生成的** scaffolding，不放松 core 里的
> trait bound（根因：wasm-bindgen issue #2833，wasm-bindgen 生成的 future 不是 Send 且
> **无法 opt-in**）。
>
> **第二堵墙原稿一个字没提**：core 里实测 **29 处 `tokio::spawn`/`tokio::time`**（分布 8 个
> 文件）。`tokio::time` 在无 timer 的 wasm 平台会 **panic**。解药是 **n0-future**（iroh 自造的
> 轮子），机械替换 `tokio::{spawn,time}` → `n0_future::{task,time}`，native 底层仍是 tokio、
> 行为不变。**这一步可脱离 iroh 独立先做、独立发版、风险接近零**，是天然的第一个技术 PR。

**正解是双通路，不是单一 facade 喂两端**：

~~~text
crates/core             业务规则与 Runtime 内部能力；不依赖 UniFFI
crates/js-api           UniFFI 投影 —— 只喂 RN（这部分你已在生产验证）
packages/bindings       ubrn 配置与生成的 RN TypeScript 产物
apps/mobile             只 import 生成后的 bindings

crates/web              ← Web 独立薄壳：只依赖 protocol 定义 + iroh Endpoint，
                          不复用 transfer 层、不走 js-api facade
apps/web                只 import crates/web 的 wasm 产物
~~~

**为什么 Web 走独立薄壳**（调研三的结论，与本节的墙互为印证）：

- 这一刀砍掉 SeaORM proxy bridge、SendWrapper、wa-sqlite/OPFS、多 tab 选主、整个 core
  塞 Worker——**约 7 周**，而砍掉的东西 Web 端本来就用不上。
- 断点续传在 Web 物理失效（关标签页 = JS 上下文销毁 = 状态全灭），不为一个用不上的能力
  搬 6789 行 transfer。
- iroh-blobs 浏览器端只有 MemStore、零持久化（README 原文 *"only the in-memory store works
  in the browser, so there is no persistence"*），2GB 文件直接 OOM，issue #84/#90 开了
  14-15 个月零 PR。**Web 端根本不该引 iroh-blobs。**

**ubrn 的成熟度（评审裁决：赌注成立但未 de-risk）**：

- Web/WASM 后端是**官方真实功能**（docs 首页原文 *"houses the bindings generators for
  React Native, **the Web**, and Node.js"*，有 `ubrn build web`，Promise/Futures 自 0.29.0-0）。
- 但作者给**整个项目**盖章 *"This project is still in early development, and **should not
  yet be used in production**."*，bus factor ≈ 1。
- 地基是 upstream 的 `wasm-unstable-single-threaded` feature，Mozilla 官方原文：
  *"likely to change or go away completely"*。
- **「同一 facade 喂 RN + Web」零生产先例**；web 发包链路本身没打通（issue #323
  `npm pack` 直接 SyntaxError，至今 open）；async 在真实 ubrn-wasm 项目仍有崩溃报告（#321）。
- **运维税你今天就在付**：`SwarmDrop-RN/patches/uniffi-bindgen-react-native@0.31.0-2.patch`
  （dunce::canonicalize 路径规范化，macOS `/Volumes` 外挂卷 firmlink 触发）。
  upstream 一片 open issue：#302/#348/#376 全是 Windows 路径与 CI——**你的桌面要出 Windows 包**。

→ **「ubrn 喂 Web」从「承重架构决策」降级为 §11 spike 的头号 kill criterion。**
  spike 要回答的不是「iroh 能不能编到 wasm」（能），而是
  **「iroh 能不能穿过 ubrn 的 UniFFI 对象模型和 async 到 wasm，且能 npm 发包」**。
  这条挂了就砍 Web 支路，**架构不受影响**（因为 Web 本来就该是独立薄壳）。

RN 侧保留原判断（**这部分已在生产验证，可信**）：

- 同一份 Rust UniFFI 导出生成 React Native TypeScript bindings；
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

> ## ⚠️ 本节经评审校正：终态对，但排序错、工具错、代价被低估
>
> **单仓是正确的长期终态**（它顺带根治 mobile-core 那条注释记录的
> "git 与 path 不能混用否则 swarm-p2p-core 撞 multiple versions" 隐患）。
> **但它不该排在 iroh spike 之前**，见 §11。以下四条会改变你对"合仓多贵"的估计：
>
> 1. **step 1 的工具选错了。** 实测：SwarmDrop-RN 工作树 111G，但 94G 是
>    `rust/mobile-core/target`、1.9G xcframework、8.6G android，**全部被 gitignore**，
>    git 只跟踪 398 个文件——所以"大仓体积"是伪问题。真正的代价是 `.git`：
>    **RN 272MiB/129 commits，主仓 260M/618 commits，subtree 全量并入后主仓 .git
>    永久翻倍到 ~530M+**，每次 clone / CI checkout 永远付费。且 subtree 不重写旧顶层
>    路径，会污染 `git log --follow` / blame / GitHub autolinks。
>    **正确工具是 `git-filter-repo --to-subdirectory-filter` 预处理再 merge。**
> 2. **step 2 的"只修路径不改行为"在双 CI 现实里不成立。** 要迁五套路径：expo prebuild
>    的仓根假设、gradle 的 `android/app/release.keystore`、metro 的
>    watchFolders/nodeModulesPaths、pnpm hoisted node-linker、ubrn 的
>    cache-workspaces/manifestPath——外加那个本地 patch。更硬的两处：
>    **两仓都是 `on: push: tags: v*` 但版本源不同**（`tauri.conf.json` vs `app.json` 的
>    expo.version + versionCode）、**发布 slug 不同**（swarmdrop vs swarmdrop-rn），
>    合仓后单个 `v*` tag 无法同时正确触发两条发布线；两仓各有独立的
>    `CLAUDE.md`/`DESIGN.md`/`PRODUCT.md`/`dev-notes`/`openspec`/`.impeccable`，**合并即撞名**。
> 3. **§10 的 crates 清单漏了 mobile-core，而它的归属是个真冲突。** mobile-core 的
>    `Cargo.toml` 自带 `[profile.release]`（`lto="thin"` / `opt-level="z"` /
>    `codegen-units=1` / `strip="symbols"`）。一旦成为主 workspace 成员，这段会被 Cargo
>    **静默忽略**（只 warn "profiles for the non-root package will be ignored"），而
>    **`lto`/`panic`/`rpath` 无法用 per-package override 挽回**（rust-lang/cargo#8264、#15262）。
>    **移动要 `opt-level=z`（体积），桌面要 `opt-level=3`（速度），单 workspace 只能有一份
>    `[profile.release]`**——"不改行为"在这里直接站不住。
> 4. **"必须合仓才能解 sync tax"是夸大的。** 更轻的解法：CI 并排 checkout 两仓 + path deps，
>    或把 core 作为 submodule 塞进 RN（`.gitmodules` 证明这个模式本项目已在用）。
>
> **判决**：单仓降级为 **spike 通过后的可延后 plumbing**；先用 path-dep 并排 checkout
> 消掉 rev-bump 循环。**别为一个未证明值得的架构预付几周不可退的接线成本。**

## 11. 实施顺序

> ## ⚠️ 本节经评审重排：原顺序是反的（critical）
>
> **原稿把 go/no-go 的 spike（step 2）排在了不可退的单仓迁移（step 1）之后。**
> 这是文档内部矛盾：§12 白纸黑字承认 iroh 在 iOS/Android/WASM 的编译体积内存可达性
> **未验证**，§11 却按已知排序。
>
> **spike 与合仓之间没有任何技术依赖**——spike 可在任一仓的临时分支或 scratch crate 里做。
> 如果先花几周合仓（filter-repo + CI 重接线 + Cargo profile 手术），spike 才发现 iroh 在
> iOS 体积不达标或中国网络不可达，**这几周全部打水漂**。
>
> 原稿还缺三件调研已定为前置的东西：**W1 的 P0 热修**（见 §0.3）、**n0-future 替换**
> （比 iroh wasm 编译更硬的前置，且可完全独立先做）、**wire-break 的迁移窗口预算**。

**修订后的顺序**（仍不代表已排期）：

1. **W1 P0 热修**（现网 libp2p 上，零重构，独立发版 v0.7.9）——见 §0.3。
   与本文所有架构决策解耦，无论后面怎么走都先做。
2. **n0-future 替换**（29 处 `tokio::spawn`/`time`，native 行为不变，独立发版）。
   零迁移风险，但它是 wasm 的硬前置。**这是第一个迁移相关、却不含迁移风险的产出。**
3. **iroh 跨端编译 spike（go/no-go 闸门，提到最前）**：桌面 / iOS / Android / WASM 分别验证。
   头号 kill criterion 是 §9 那条（iroh 能否穿过 ubrn 的 UniFFI 对象模型 + async 到 wasm
   且能发包）；**iroh-on-iOS/Android 经 uniffi 交叉编译（noq/ring 进 xcframework/.a）无先例，
   是整个项目的关键路径闸门**。同期实测中国三网的 direct/relay 比例。
4. **桌面与移动锁步替换 libp2p 身份、连接与配对**——旧 PeerId 不做静默信任迁移，
   要求显式重新配对。**移动锁步是平行关键路径不是尾巴**（mobile-core 用 git rev 钉住
   `swarm-p2p-core`，删 `libs/` 那刻即编译失败；双商店审核是不可压缩的日历）。
   **wire-break 策略必须在动 core 之前定死**，不能用"双端灰度 2d"一笔带过。
5. **落地邀请链接/二维码配对，删除 DHT 配对码**（同时结构性根治 §2.2 的 P0）。
6. **desktop daemon、CLI/TUI 和本机 IPC**。
7. **最后做范围很窄、可关闭的 Web 临时端 PoC**（带 kill switch：发布 7 天后取件量
   < 桌面传输量 5% 就砍，代码留仓库当 demo）。
8. **单仓迁移**：降级为可延后 plumbing，在 spike 通过后择机做（见 §10 校正）。

libp2p 与 Iroh 的 wire/身份体系不兼容，不要假设可无感混用。

## 12. 仍需 PoC / 未决项

- Iroh 在 iOS、Android、WASM 上的实际编译、体积、内存和网络可达性；
- 中国三大运营商、企业网、校园网、公共 Wi-Fi 的 UDP 直连率和 relay 体验；
- 自建 relay 的地域、成本、可达性、合规和监控；
- Web deep link 是否可靠保留 fragment；
- 浏览器大文件传输是否使用自研精简 transfer，而非 iroh-blobs（**评审已定：不用
  iroh-blobs，见 §9**）；
- irpc-iroh 与手写 QUIC control stream 的取舍；
- macOS LaunchAgent / SMAppService 的打包与权限路径；
- IPC 协议版本、事件背压、socket/pipe 安全与多 profile；
- Iroh EndpointId 与旧 PeerId 配对记录的迁移 UX。

**评审补充的未决项（比上面几条更硬，且有的可以先做）**：

- **core 的 29 处 tokio 全量替换 n0-future**——比 iroh wasm 编译更硬的前置，**且可完全
  独立先做、零迁移风险**（§11 step 2）；
- **全 core 的条件 Send 界改造**（~14 个 async trait + 全部 impl + 所有 `Arc<dyn ...>`
  字段）——贯穿平台中立核心的横切手术，**必须与 iroh 迁移同期做，不能拖到 Web 端才发现**；
- **iroh-on-iOS/Android 经 uniffi 交叉编译**（noq/ring 进 xcframework/.a）——**无先例**，
  是整个项目的关键路径闸门；
- **ubrn 能否穿过 UniFFI 对象模型 + async 到 wasm 并 npm 发包**（§9 的头号 kill criterion）；
- **wire-break 的迁移窗口**：iOS 强制更新在物理上不存在（审核 1-7 天 + 用户自主更新，
  长尾以月计），**必然存在数周到数月的窗口：桌面新版与移动旧版跨网配对完全断掉**；
- **存量配对怎么处理（需显式签字）**：v0.7.8 的每一个现存配对都是通过已被判定攻破的流程
  建立的，而 `PairedDeviceInfo::new` 硬编码 `trust_confirmed = true`。二选一：
  (a) 升级后全部打回 false 逼用户重新确认；(b) 显式写下"接受存量配对可能已被 MITM，
  不做处理"并签字。**不要把"已配对设备不受影响"当优点写。**
- **iOS/Android 的 mDNS 今天其实是死的**：`SwarmDrop-RN` 的 entitlements 没有
  `com.apple.developer.networking.multicast`、Info.plist 没有
  `NSLocalNetworkUsageDescription`/`NSBonjourServices`、Android manifest 没有
  `CHANGE_WIFI_MULTICAST_STATE`。只是被 DHT rendezvous 扛着 100% 流量所以没暴露。
  **Apple 的 Multicast Entitlement 是人工裁量、周期以周计——不管最后走哪条路，先排队。**
- 存量用户数据迁移 / 五端回归 QA / i18n（主仓 3 locale + RN 独立 2 locale）/ docs 站重写 /
  **demo 素材重录**（`video/` + `e2e/` 录的全是旧 6 位码流程，配对一改全作废）。

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
