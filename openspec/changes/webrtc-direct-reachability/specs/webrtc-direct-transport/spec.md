# webrtc-direct-transport

## ADDED Requirements

### Requirement: 浏览器经 webrtc-direct 无 TLS 直连

浏览器 SHALL 能经 webrtc-direct 传输（地址 `/ip4/../udp/../webrtc-direct/certhash/<h>`，裸 IP + 自签证书哈希，无需域名/CA/TLS 证书）直连自托管 helper/bootstrap 节点与桌面端点，实现跨网可达。远端身份认证 SHALL 由 DTLS 之上的 Noise 握手 + Ed25519 peer-id 完成，`certhash` 仅作带外绑定进 Noise prologue，**不得** 作为身份判定或授权依据。

#### Scenario: 浏览器跨网直连自托管 helper

- **WHEN** 浏览器节点拨一个自托管 helper 的 webrtc-direct 地址（裸公网 IP + certhash）
- **THEN** 不经域名/CA，DTLS + Noise 握手成功建立连接，远端 peer-id 与 NodeId 一致

#### Scenario: 浏览器 ↔ 桌面端点直连

- **WHEN** 桌面端点在 invite 中携带自身 webrtc-direct 地址，浏览器解码后据此拨号
- **THEN** 浏览器与桌面端到端 webrtc-direct 直连建立，无需中继中转

### Requirement: 证书持久化固定 certhash

节点 SHALL 支持持久化整张 webrtc-direct 证书 PEM（含私钥），跨重启复用以固定 certhash——桌面存 keychain/Stronghold、helper/bootstrap 存服务器数据目录。持久化单位 SHALL 是整张证书而非仅密钥（certhash = SHA-256(证书 DER)，重建密钥会因随机 SAN 产生不同 certhash）。已配置持久化证书时 SHALL **不** 发出「certhash 重启失效」告警。

#### Scenario: certhash 跨重启稳定

- **WHEN** 节点以同一份持久化证书 PEM 先后两次启动
- **THEN** 两次 webrtc-direct 地址的 certhash 段完全一致，此前分发的该地址仍可拨

#### Scenario: 未配置证书时告警

- **WHEN** 节点未配置持久化证书、启动时随机生成
- **THEN** 记录一条明确告警说明该 certhash 不跨重启存活，提示配置持久化

### Requirement: NodeId 为长期锚点，certhash 地址为可失效 hint

分享物（invite/地址）SHALL 以 `NodeId` 为长期身份锚点；webrtc-direct/certhash 地址 SHALL 作为机会主义 hint 携带。拨号 SHALL 先尝试 hint，失败时回落按 `NodeId` 经 DHT/presence（`OnlineRecordLookup`）重解析当前 dialable 地址。webrtc-direct 地址 SHALL 被纳入节点的 `dialable()` 地址集并被 presence announce（不因 loopback/unspecified/circuit 过滤规则被误杀）。

#### Scenario: hint 陈旧时按 NodeId 重解析

- **WHEN** invite 中的 certhash 地址已因换证/换网失效，受邀方据此拨号失败
- **THEN** 受邀方按 NodeId 经 DHT presence 重解析拿到当前 webrtc-direct 地址并成功连接，无需重发邀请

#### Scenario: 换证后无需重发邀请

- **WHEN** 某端点更换 webrtc-direct 证书（certhash 变），其 presence 记录随后 announce 新地址
- **THEN** 已配对对端按不变的 NodeId 自动重解析到新 certhash 地址，原配对关系不失效
