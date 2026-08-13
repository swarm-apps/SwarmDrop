## ADDED Requirements

### Requirement: 地址形态与解析

`webtransport-p2p` SHALL 按 libp2p WebTransport spec 处理 multiaddr：可拨地址形如
`/ip4/…/udp/<port>/quic/webtransport/certhash/<h1>[/certhash/<h2>…][/p2p/<id>]`，
监听地址形如 `/ip4/…/udp/<port>/quic/webtransport`（**不带** certhash 与 `/p2p` 段）。
certhash 编码 MUST 与官方逐位一致（multihash，SHA2-256）。

#### Scenario: 解析可拨地址

- **WHEN** 传入 `/ip4/1.2.3.4/udp/4004/quic/webtransport/certhash/<h1>/certhash/<h2>`
- **THEN** 系统 MUST 产出 socket `1.2.3.4:4004` 与**有序保留**的 certhash 列表 `[h1, h2]`

#### Scenario: 解析监听地址

- **WHEN** 传入 `/ip4/0.0.0.0/udp/4004/quic/webtransport`
- **THEN** 系统 MUST 接受它，产出 socket `0.0.0.0:4004`（通配 IP 与端口 0 均合法）

#### Scenario: 拒绝带 certhash 的监听地址

- **WHEN** 传入的监听地址含 `/certhash` 段
- **THEN** 系统 MUST 拒绝——本机指纹由本机证书决定，写进监听地址是冗余且可能与实际证书矛盾

#### Scenario: 不认领非本 transport 的地址

- **WHEN** 传入 `/ip4/…/udp/…/quic-v1/p2p/<id>`（QUIC 但无 `/webtransport` 段）
- **THEN** 系统 MUST 返回「不认领」而非报错，交由地址分派链上的其他 transport 处理

### Requirement: HTTP endpoint 路径拦截

服务端 SHALL 只接受 `:path` 为 `/.well-known/libp2p-webtransport` 且 `type` 查询参数为
`noise` 的 CONNECT 请求，其余一律以 HTTP 404 拒绝，**且不得进入后续的 libp2p 处理路径**。

#### Scenario: 合法路径被接受

- **WHEN** 客户端 CONNECT 到 `/.well-known/libp2p-webtransport?type=noise`
- **THEN** 服务端 MUST 接受该会话并将其交给 libp2p 层

#### Scenario: 路径不匹配被拒

- **WHEN** 客户端 CONNECT 到 `/` 或任何其他路径
- **THEN** 服务端 MUST 以 404 拒绝，且该连接 MUST NOT 产生任何 `TransportEvent`

#### Scenario: 缺少或错误的 type 参数被拒

- **WHEN** 客户端 CONNECT 到 `/.well-known/libp2p-webtransport` 但 `type` 缺失或不是 `noise`
- **THEN** 服务端 MUST 以 404 拒绝

### Requirement: 监听与通告地址

`listen_on` SHALL 绑定指定 UDP socket 并为**每个可通告的具体地址**发出
`TransportEvent::NewAddress`。绑定在通配 IP（`0.0.0.0` / `::`）时，系统 MUST 展开成本机具体
网卡地址后再通告——通配地址通告出去对端无法拨号。展开失败时 MUST 原样通告通配地址并留下
warn 级日志，不得静默什么都不报。

#### Scenario: 绑定具体地址

- **WHEN** 监听 `/ip4/192.168.1.5/udp/4004/quic/webtransport`
- **THEN** 系统 MUST 通告恰好一条地址，且该地址携带当前全部通告 certhash

#### Scenario: 绑定通配地址

- **WHEN** 监听 `/ip4/0.0.0.0/udp/4004/quic/webtransport`
- **THEN** 系统 MUST 为每个 IPv4 网卡通告一条地址，每条都保留绑定端口且不含通配 IP

#### Scenario: 关闭监听器

- **WHEN** 调用 `remove_listener`
- **THEN** 系统 MUST 停止接受新连接并释放 UDP 端口，且既有连接的关闭由各自的 muxer 负责

### Requirement: Noise 认证握手

客户端 SHALL 在 CONNECT 之后在**第一条 bidi 流**上发起 Noise 握手，并且 MAY 不等待服务端的
CONNECT 响应即开始。该握手**只用于身份认证，不用于加密**——握手完成后该流即关闭，后续子流
是 WebTransport 上的明文，保密性由 QUIC-TLS 承担。本 transport MUST NOT 设置 Noise prologue
（身份与信道的绑定由 `webtransport_certhashes` 扩展承担，见
`webtransport-certificate-lifecycle`）。

#### Scenario: 握手成功

- **WHEN** 双方在第一条流上完成 Noise 握手
- **THEN** transport MUST 产出对端的 `PeerId`，并将该流关闭，不作为子流交给上层

#### Scenario: 握手超时

- **WHEN** 从会话建立到 Noise 握手完成超过配置的握手超时
- **THEN** 该次 upgrade MUST 失败并关闭底层会话，不得留下悬挂的连接或任务

#### Scenario: 拨号时对端 PeerId 不符

- **WHEN** 拨号地址带 `/p2p/<id>` 但握手得到的 `PeerId` 与之不同
- **THEN** upgrade MUST 失败

### Requirement: 子流映射

一条 WebTransport bidi 流 SHALL 直接对应一条 libp2p 子流，**不附加任何 framing、标签或握手
消息**。`StreamMuxer` 的入站 / 出站 / 关闭分别映射到会话的 `accept_bi` / `open_bi` / `close`。

#### Scenario: 打开出站子流

- **WHEN** 上层请求一条出站子流
- **THEN** 系统 MUST 开一条 WebTransport bidi 流并原样暴露为可读可写的字节流

#### Scenario: 接受入站子流

- **WHEN** 对端开一条 bidi 流
- **THEN** 系统 MUST 将其作为入站子流交给上层，首字节即为上层协议数据

#### Scenario: muxer 被丢弃时会话必须关闭

- **WHEN** `StreamMuxer` 被 drop 而未走正常关闭流程
- **THEN** 底层 WebTransport 会话 MUST 被关闭，不得泄漏空转的后台任务

### Requirement: 拨号

transport SHALL 支持从 native 侧拨 `/quic/webtransport` 地址，且**不要求本机已在监听**。
拨号方 MUST 把地址中的全部 certhash 作为「愿意接受的证书哈希集合」传给 Noise 层验证。

#### Scenario: 拨通并认证

- **WHEN** 拨一个含正确 certhash 的地址
- **THEN** 连接 MUST 建立、Noise 握手 MUST 成功、`PeerId` MUST 与服务端身份一致

#### Scenario: 未监听时拨号

- **WHEN** 本机没有任何 WebTransport 监听器
- **THEN** 拨号 MUST 照常可用（拨号方使用独立的临时端口）

### Requirement: crate 边界

`crates/webtransport-p2p` SHALL 不依赖任何 swarmdrop crate，且公共 API MUST NOT 出现
`wtransport` 的类型——它长在本仓只为借 `crates/net` 做集成测试，稳定后要 subtree split 出去
独立发布，任何反向依赖或底层类型泄漏都会堵死这条路。该 crate 为 native-only，不参与 wasm
target 编译；浏览器侧由 `crates/net` 按 target 分派到上游的 `libp2p-webtransport-websys`。

#### Scenario: 依赖方向检查

- **WHEN** 在 `crates/webtransport-p2p/src` 下检索 `swarmdrop`
- **THEN** MUST 零命中

#### Scenario: wasm 门禁不受影响

- **WHEN** 执行 `./scripts/check-wasm.sh --clippy`
- **THEN** MUST 通过——新 crate 不进 wasm 依赖树，`crates/net` 的 wasm 分支只依赖
  `libp2p-webtransport-websys`

### Requirement: 日志 target 放行

桌面与移动两份 `DEFAULT_FILTER` SHALL 各自单列 `webtransport_p2p`、`wtransport`、`quinn`
三个 target。`EnvFilter` 按字符串前缀匹配，三者互不为前缀也不以 `swarmdrop` 开头，漏掉任一
条会导致该层日志在生产构建里**一条都不出现**。

#### Scenario: 过滤器覆盖三个 target

- **WHEN** 用默认 filter 构造 `EnvFilter` 并检查三个 target 的 debug 级事件
- **THEN** 三者 MUST 全部通过，且该断言 MUST 由测试看守（照抄
  `default_filter_passes_the_targets_we_depend_on`）
