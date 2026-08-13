## ADDED Requirements

### Requirement: 中转连接在可达局域网直连时必须升级

内核 SHALL 在「与某对端的**全部**连接均为 `PathKind::Relayed`、且已获知该对端的私网可达地址」
时，主动向这些地址发起拨号以建立直连；升级成功后最优路径 SHALL 变为 `PathKind::Local`，
并 SHALL 发出 `NetEvent::PathChanged`。

私网地址的来源 SHALL 至少包含 identify 的 `listen_addrs`（对端自报），**SHALL NOT** 只依赖
mDNS——移动平台可能整个禁用组播，只挂在 mDNS 上等于在那些平台永远不升级。

候选地址 SHALL 排除 loopback、link-local、公网地址与 circuit 地址：前两类对端不可达，公网归
打洞路径，circuit 正是要摆脱的东西（局域网 helper 派发的 circuit 地址前半段也是私网，换一条
中继不算升级）。

候选数量若需设上限，SHALL **按传输分别计数**，使每种传输都保留代表；**SHALL NOT** 对整个
地址表笼统取前 N 个。原生端同时监听 tcp / quic-v1 / webrtc-direct，各自再乘以网卡数与
IPv4/IPv6，笼统截断会砍掉排在末位的 webrtc-direct——而那是浏览器够到局域网内原生端的唯一
传输（浏览器拨不了裸 TCP/QUIC）。

#### Scenario: 同网段设备经中继连上后升级为直连

- **WHEN** 两台同一局域网的已配对设备先经公网 relay 建立连接，随后 identify 到达并携带对端的
  私网监听地址
- **THEN** 内核 SHALL 向该私网地址发起拨号，建立成功后最优路径 SHALL 为 `PathKind::Local`

#### Scenario: mDNS 不可用时仍能升级

- **WHEN** 本端或对端的 mDNS 因平台限制完全不工作，但双方在同一网段且 relay 连接已建立
- **THEN** 升级 SHALL 仍然发生（走 identify 自报地址那条来源）

#### Scenario: 跨网设备不因 LAN 升级失败而失去打洞机会

- **WHEN** 对端自报的私网地址对本端不可达（两端不在同一网络），LAN 升级拨号失败
- **THEN** 打洞升级 SHALL 仍在**同一轮** identify 中被发起，**SHALL NOT** 因 LAN 升级占用在途
  标记而被跳过

#### Scenario: 浏览器与同网段原生端之间升级为 webrtc-direct 直连

- **WHEN** 浏览器与同一局域网内的原生端（桌面 / 移动）经 relay 连上，原生端 identify 自报的
  地址里含私网 `webrtc-direct` 监听地址
- **THEN** 该地址 SHALL 出现在升级候选中（不得因候选上限被截掉），升级成功后路径 SHALL 为
  `PathKind::Local`、传输 SHALL 报告为 WebRTC Direct

#### Scenario: 已有非中转路径时不重复升级

- **WHEN** 某对端已存在 `Local` 或 `Direct` 路径的连接
- **THEN** 内核 SHALL NOT 为其发起 LAN 升级拨号

### Requirement: mDNS 初始化失败不得中断节点启动

mDNS 是可选的发现加速手段。其 behaviour 构建失败时内核 SHALL 记录警告并在无 mDNS 的情况下
继续启动，**SHALL NOT** panic 或让 `bind()` 返回错误。

#### Scenario: 平台不允许绑定 5353

- **WHEN** 运行环境拒绝绑定 UDP 5353 或缺少组播接口
- **THEN** `Endpoint::bind()` SHALL 成功返回，节点 SHALL 正常运行，局域网发现 SHALL 降级为
  identify 驱动的升级路径

### Requirement: 移动端必须具备 mDNS 所需的平台条件

移动端 SHALL 声明各平台组播所需的配置：iOS SHALL 提供 `NSLocalNetworkUsageDescription` 与
`NSBonjourServices`（含 libp2p 使用的 `_p2p._udp`）；Android SHALL 声明
`CHANGE_WIFI_MULTICAST_STATE` 并在节点运行期间持有 `MulticastLock`。

组播锁的生命周期 SHALL 与节点绑定（节点运行 ⇔ 持锁）——持锁期间 Wi-Fi 芯片不进省电态，
节点停止后继续持有只是白耗电。

#### Scenario: 节点启停时组播锁跟随

- **WHEN** 用户启动节点
- **THEN** Android 侧 SHALL 取得 MulticastLock；**WHEN** 节点停止，**THEN** SHALL 释放它

#### Scenario: 平台无原生实现时不阻断启动

- **WHEN** 在 iOS 或原生模块缺席的构建上调用组播锁 API
- **THEN** 调用 SHALL 静默 no-op，**SHALL NOT** 抛错中断节点启动

### Requirement: 链路详情必须可从上层读到

`Device` SHALL 携带当前最优连接的链路详情：传输协议、远端地址、以及经中继时的中继身份。
该详情 SHALL 与 `connection` 取自**同一次**连接快照。

设备离线、或内核尚未报告过连接地址时，链路详情 SHALL 为空——**SHALL NOT** 沿用上一次连接的
地址，那会让人对着一条已失效的链路排查。

传输协议在地址中读不出时 SHALL 为空值（入站中继连接的 `send_back_addr` 只有 `/p2p/<src>` 一段），
**SHALL NOT** 以任何默认值填充。

#### Scenario: 打洞连接不得被报告为其承载信令的传输

- **WHEN** 连接的远端地址是 `<relay-addr>/p2p-circuit/webrtc/p2p/<peer>`（打洞，信令经 relay）
- **THEN** 传输协议 SHALL 为 WebRTC，**SHALL NOT** 为 circuit 前半段的 TCP 或 QUIC

#### Scenario: 纯中继连接报告承载中转字节的传输

- **WHEN** 连接的远端地址是 `/ip4/…/tcp/…/p2p/<relay>/p2p-circuit/p2p/<peer>`
- **THEN** 传输协议 SHALL 为 TCP，中继身份 SHALL 为 `<relay>`（而非末位的 `<peer>`）

#### Scenario: 断连宽限期内不给出过期链路

- **WHEN** 已配对设备断连但 presence 仍在宽限期内呈现「在线」
- **THEN** `connection` MAY 由 mDNS 地址推断保留，链路详情 SHALL 为空
