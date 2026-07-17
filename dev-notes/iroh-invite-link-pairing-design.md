# 基于 Iroh 的邀请链接与二维码配对设计

> 状态：提案  
> 日期：2026-07-16  
> 前置条件：网络层完成从 libp2p 到 Iroh 的迁移，并完成桌面端、移动端与浏览器端的连通性验证。

## 1. 概要

将现有“6 位配对码 → DHT 查询 → 连接 PeerId”的临时发现方式，替换为一次性、可验证的 **邀请链接（Invite Link）**。

二维码不是另一种配对协议，而只是同一邀请链接的图形编码：

- 有摄像头：扫描二维码；
- 无摄像头：点击、粘贴或手动打开同一链接；
- 链接优先尝试唤起已安装的 SwarmDrop；无法唤起时进入网页临时端。

Iroh 的 EndpointId 是设备的稳定加密身份。邀请链接携带或解析出发起方的 EndpointId 与可连接地址，接收方连接后必须验证实际远端身份与邀请中声明的身份一致；双方完成一次性能力凭据和用户确认后，才写入长期配对记录。

## 2. 背景与问题

当前配对码方案把 6 位数字映射到 DHT 记录。它适合早期验证，但不适合作为长期信任建立机制：

- 6 位空间很小，容易被枚举、碰撞或抢注；
- DHT 记录只能帮助“找到一个候选节点”，不能证明该节点就是用户面对面确认的设备；
- 记录覆盖、过期与网络抖动会造成配对失败或静默误连；
- 配对码不适合自然地在网页、桌面端和移动端之间传播。

迁移到 Iroh 后，地址发现与实际连接由 Iroh 的 EndpointId、relay、直接路径及 discovery 机制处理。SwarmDrop 仍需定义自己的业务配对协议，但不再需要用低熵数字作为身份发现的唯一锚点。

## 3. 目标与非目标

### 目标

- 用一个可分享链接完成桌面端、移动端和网页端的临时配对入口。
- 二维码与链接完全等价，二者解析到同一份邀请数据。
- 建立可验证、可过期、一次性使用的配对能力，避免 DHT 码被猜中后直接建立信任。
- 配对完成后仅依赖稳定设备身份重连，不维护公共设备目录。
- 保留“仅局域网”产品模式，并保证该模式不会因 fallback 意外走公网 relay。
- 让协议与宿主解耦：核心实现可由桌面 Tauri、React Native 和 WASM 网页端复用。

### 非目标

- 不把 Iroh discovery 当作 SwarmDrop 的联系人目录或陌生人搜索服务。
- 不要求浏览器实现 UDP 打洞或完全复刻原生端的直连能力。
- 不承诺用旧 libp2p PeerId 自动迁移为 Iroh 信任关系。
- 不在本设计中定义完整的文件传输 UI、断点续传或文件授权协议。

## 4. 术语

| 术语 | 含义 |
| --- | --- |
| EndpointId | Iroh 端点稳定的 Ed25519 公钥身份。 |
| EndpointAddr | 某个 EndpointId 当前可用的 relay URL、直连地址等可连接信息。 |
| PairInvite | 一次性配对邀请的自包含载荷。 |
| capability | 至少 128 bit 的随机一次性能力凭据；只保存其哈希。 |
| 邀请链接 | 承载 PairInvite 的用户可分享 URL。 |
| 配对记录 | 用户确认后持久化的可信设备身份及本地展示信息。 |

## 5. 总体流程

~~~mermaid
flowchart LR
  A[发起设备] --> B[生成 PairInvite]
  B --> C[同一邀请链接]
  C --> D[二维码]
  C --> E[点击或粘贴链接]
  D --> F[接收设备或网页端]
  E --> F
  F --> G[Iroh 连接发起方 EndpointId]
  G --> H[交换 capability 和双方确认]
  H --> I[写入配对记录]
  I --> J[后续按 EndpointId 重连]
~~~

配对链接只用于建立信任。配对成功后，设备间的后续发现与连接由 Iroh 的地址缓存、DNS/Pkarr、relay 或本地 discovery 等机制协助完成，不需要再次输入配对码。

## 6. 邀请链接与二维码

### 6.1 规范形式

推荐的外部链接形式：

~~~text
https://swarmdrop.app/i#<base64url-encoded-pair-invite>
~~~

其中 fragment 不会随普通 HTTP 请求发送给站点服务器，可降低邀请内容被 CDN、访问日志或 Referer 收集的概率。落地页在浏览器本地读取 fragment，验证并决定：

1. 尝试通过 Universal Link / App Link 打开原生应用；
2. 桌面端尝试自定义协议，例如 swarmdrop://invite；
3. 未安装应用或打开失败时，进入网页临时端。

二维码编码的必须是**完整的同一条邀请链接**。UI 同时提供“复制链接”“分享”“粘贴链接”入口；不应要求没有摄像头的设备手输长链接。

### 6.2 兼容性约束

不同平台对深链和 fragment 的保留行为存在差异，必须先做真实设备 PoC。若某个平台无法可靠传递 fragment，可引入短 token 兼容层：

~~~text
https://swarmdrop.app/i/<opaque-token>
~~~

该 token 必须是高熵、一次性、短 TTL 的能力引用，并在安全的 rendezvous/邀请存储中解析；它不是可枚举的 6 位码，也不能重新成为身份信任依据。

## 7. PairInvite 数据模型

PairInvite 使用明确版本化的二进制序列化（推荐 postcard 或 CBOR），再以 base64url 编码。推荐逻辑结构如下：

~~~rust
struct PairInvite {
    version: u16,
    invite_id: [u8; 16],
    capability: [u8; 32],
    inviter_endpoint_id: EndpointId,
    inviter_addr: EndpointAddr,
    issued_at: UnixTime,
    expires_at: UnixTime,
    transport_policy: TransportPolicy,
    display_hint: DeviceDisplayHint,
    signature: Ed25519Signature,
}

enum TransportPolicy {
    Auto,
    LocalOnly,
}
~~~

字段约束：

- capability 使用密码学安全随机数，长度不低于 128 bit；推荐 256 bit。
- expires_at 默认 5 分钟，超时后发起端立刻停止接受该邀请。
- signature 由 inviter_endpoint_id 对除签名字段外的规范化内容签名，防止链接字段被替换。
- inviter_addr 是连接提示，不是身份；最终身份只认连接握手得到的 EndpointId。
- display_hint 仅用于展示设备名称、平台或头像，不参与授权决策。

发起端只持久化 hash(capability)、invite_id、过期时间和使用状态；绝不把明文 capability 写入日志、崩溃报告或长期数据库。

## 8. 配对协议

控制通道由 SwarmDrop 自定义，建议协议名为：

~~~text
com.yexiyue.swarmdrop.pairing/1
~~~

实现可以使用 irpc + irpc-iroh，但请求、响应、版本兼容及业务语义归 SwarmDrop 所有。配对不是“连上即成功”，必须通过以下顺序完成：

~~~mermaid
sequenceDiagram
  participant A as 发起方
  participant B as 接收方
  B->>B: 解析、验签、检查 TTL
  B->>A: Iroh connect(inviter_endpoint_id, inviter_addr)
  A-->>B: Iroh 握手完成
  B->>B: 验证 remote EndpointId 等于 inviter_endpoint_id
  B->>A: PairHello(invite_id, capability, receiver_endpoint_id)
  A->>A: 校验 capability 哈希、TTL、未使用
  A-->>B: PairOffer(发起方展示信息、确认摘要)
  A->>A: 用户确认接收设备
  B->>B: 用户确认发起设备
  B->>A: PairAccept(绑定的确认摘要)
  A-->>B: PairCommit(配对记录摘要)
  A->>A: 消费 invite，写入配对记录
  B->>B: 写入配对记录
~~~

必须执行的安全检查：

1. 本地验证序列化格式、版本、签名及过期时间。
2. Iroh 连接完成后，验证实际远端 EndpointId 等于邀请中的 inviter_endpoint_id。
3. capability 必须匹配、未使用且未撤销；匹配后只允许一次成功提交。
4. 用户界面必须展示对方设备名称、平台和短身份指纹，并要求双方显式确认。
5. PairAccept 与 PairCommit 绑定 invite_id、双方 EndpointId 和本次会话摘要，避免消息跨会话重放。
6. 任一步失败均不写入长期信任记录，并向用户展示可理解的失败原因。

## 9. 配对后的持久化与重连

配对成功后的最小记录如下：

~~~rust
struct PairedDevice {
    endpoint_id: EndpointId,
    trust_created_at: UnixTime,
    display_name: String,
    platform: Platform,
    fingerprint: ShortFingerprint,
    last_seen_at: Option<UnixTime>,
    user_label: Option<String>,
}
~~~

不持久化把对方固定到某个 IP、端口或 relay URL；这些地址会变化。重连时以 EndpointId 为锚点，交给 Iroh discovery、缓存地址和连接策略解析可达路径。用户移除设备时，应删除本地信任记录和地址缓存，并拒绝该设备继续使用旧会话恢复授权。

## 10. 网络模式与 Web 边界

| 模式 | 原生端 | 浏览器端 |
| --- | --- | --- |
| Auto | 允许 Iroh 选择直连、relay 与可用 discovery。 | 可参与邀请和传输，但当前按 relay-only 设计。 |
| LocalOnly | 只允许本地地址/本地 discovery；禁止公共 relay、公共 DNS/Pkarr 与公网 fallback。 | 不承诺支持；应提示改用已安装的原生应用。 |
| 临时网页端 | 不写入默认长期配对记录，除非用户明确选择保存。 | 使用 WASM Iroh、relay 和内存态会话；关闭页面即失效。 |

浏览器端不是原生 Iroh 的等价网络环境：浏览器无法使用原生 UDP socket，不能依赖 QUIC 打洞或严格局域网直连。网页端应定位为“无需安装即可临时接收/发送”，而不是 LocalOnly 模式的替代品。若使用 iroh-blobs，浏览器侧存储应先按内存实现评估大文件限制和页面关闭后的恢复策略。

## 11. 交互设计

### 发起方

1. 在“添加设备”选择“创建邀请链接”。
2. 选择网络策略：自动或仅局域网。
3. 展示二维码、复制链接和系统分享按钮，并显示 5 分钟倒计时。
4. 有设备请求加入时，展示其名称、平台、短指纹，点击“确认配对”。
5. 成功后显示设备已保存；取消、超时或使用后立即使链接失效。

### 接收方

1. 扫码、点击或粘贴邀请链接。
2. 显示即将连接的设备名称、平台、短指纹和网络策略。
3. 完成连接后要求用户确认；成功后可选择“保存为我的设备”。
4. 网页端默认仅建立临时会话，并清楚提示能力边界。

## 12. 迁移与废弃

- Iroh EndpointId 与 libp2p PeerId 是不同身份体系，不能把旧配对记录静默视为已迁移信任。
- 首个 Iroh 版本应让已有用户重新配对，或在用户面对面确认的前提下执行显式迁移。
- 移除旧的“6 位码 → DHT 值”发布、查询、TTL 清理和相关前端输入流程。
- DHT 若仍被保留，只能承担 Iroh 地址发现等基础网络能力；不得作为低熵配对码的信任来源。

## 13. 实施阶段与验收

### 阶段一：协议与原生端 PoC

- 实现 PairInvite 编解码、签名、过期和一次性 capability 校验。
- 用 Iroh EndpointId 完成双端连接与身份一致性验证。
- 实现双方确认和配对记录写入。
- 验收：邀请被篡改、过期、重复使用、身份不匹配均无法写入信任记录。

### 阶段二：链接、二维码与深链

- 实现同链接的二维码、复制和粘贴入口。
- 在 iOS、Android、macOS、Windows、Linux 上验证 App/Universal Link 与 fragment 传递。
- 验收：没有摄像头的设备可只靠复制/粘贴完成配对；二维码与链接行为一致。

### 阶段三：网页临时端

- 验证 WASM Iroh、irpc-iroh 与 iroh-blobs 在目标浏览器中的构建和 relay 连通。
- 明确内存、文件大小、后台切换和关闭页面后的行为。
- 验收：网页可完成 Auto 模式下的临时配对/传输；LocalOnly 明确拒绝并给出原生端引导。

### 阶段四：旧方案下线

- 删除 6 位配对码和 DHT 映射的用户入口及后端写入。
- 编写重新配对说明与回归测试。
- 验收：代码库中不再存在将低熵配对码直接映射为可信设备身份的路径。

## 14. 待验证事项

- Iroh 当前版本在各原生平台和 WASM 的 API 差异、relay 配置和地址编码方式。
- 各平台深链是否稳定保留 URL fragment；如不稳定，短 token 服务的最小部署和隐私策略。
- irpc-iroh 的 WASM 互操作性是否足以承担配对控制协议；必要时保留小型自定义控制帧实现。
- 邀请链接长度与二维码扫描可靠性的平衡；过长时优先压缩编码或使用高熵短 token，不降低安全熵。

## 15. 相关文档

- [现有配对实现](pairing-implementation.md)
- [Rendezvous 与配对风险调研](../rendezvous-recon-2026-07.md)
- [Core、桌面与移动端边界](../architecture/core-desktop-mobile-boundaries.md)
- [未来 OpenSpec 候选项](../architecture/future-openspec-candidates.md)
