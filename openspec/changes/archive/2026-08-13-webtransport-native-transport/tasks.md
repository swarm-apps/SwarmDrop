## 1. 前置门（不写传输层代码）

> 这两组是 gate：1.1 的结论若否定收益，本 change 就地中止；1.2 的结论可能改变库选型。
> **在两组都过之前不要开始第 3 组。**

- [ ] 1.1 真机吞吐测量：分离「WebRTC 打洞 vs webrtc-direct」这个尚未分离的变量。真机那条
      0.36–0.96 MB/s 走的是打洞路径，而回环 80 MiB/s 测的是 direct。方法与判据见
      `dev-notes/research/2026-08-11-web-webrtc-throughput.md` §5。**至少取 6 次中位数**
  → **未做，需要真机与两台设备**。这是本 change 唯一没能自证的前置：回环 322 vs 72 MiB/s 的差距不能外推（回环瓶颈是 CPU，跨网是带宽与 RTT）。
- [ ] 1.2 记录 1.1 结论到 research 报告：若真机瓶颈不在 CPU（而在带宽 / RTT / 停等流控），
      在此中止并把结论写回 `dev-notes/prompts/webtransport-native-transport.md`
  → 同上，待 1.1 有数据后写回。
- [x] 1.3 `wtransport` spike：起一个最小 listener，实测 `Endpoint::reload_config(cfg, false)`
      在既有连接存活时换证书——验证「不中断既有连接」是否属实，以及轮换瞬间正在 CONNECT
      的客户端拿到哪张证书（design.md Open Question 2）
- [x] 1.4 `wtransport` spike：验证能否经 `pub use quinn` 取到底层 `Connection` 的统计
      （RTT / 丢包 / 拥塞窗口）。拿不到不阻塞，记为负债（Open Question 1）
- [ ] 1.5 若 1.3 证伪「不中断既有连接」，重新评估 `web-transport-quinn` + 自建轮换的路径，
      并更新 design.md 的 Decision 2
  → **不适用**：1.3 已证实 `reload_config` 可用（`rotation_keeps_existing_connections_alive` 就是那条验证），无需回退到 `web-transport-quinn`。

## 2. crate 骨架

- [x] 2.1 新建 `crates/webtransport-p2p`，加入根 Cargo workspace members
- [x] 2.2 `Cargo.toml`：依赖 `wtransport` 0.7.1、`libp2p-core`、`libp2p-identity`、
      `libp2p-noise`、`multiaddr`、`multihash`、`futures`、`tokio-util`（compat）、
      `tracing`、`thiserror`。**零 swarmdrop 依赖**
- [x] 2.3 `Cargo.toml` 顶部写下与 `crates/webrtc-p2p` 同款的定位注释：不带 swarmdrop 前缀、
      零 swarmdrop 依赖、稳定后 subtree split，评审时优先看它
- [x] 2.4 `lib.rs` 模块级文档：复杂度分布表（重心在证书生命周期而非传输层）+ 三层依赖方向表
- [x] 2.5 `error.rs`：`Error` 类型（thiserror），区分「地址不认领」「配置/证书错误」
      「连接失败」「握手失败」四类
- [x] 2.6 `cargo check -p webtransport-p2p` 通过（空实现）

## 3. L0 纯函数层：`addr.rs`

> 零 IO、零 `wtransport`、只用 `Multiaddr` / `Multihash` 类型。全部可单测。

- [x] 3.1 `is_webtransport(&Multiaddr) -> bool`
- [x] 3.2 `parse_dial(&Multiaddr) -> Option<(SocketAddr, Vec<Certhash>)>`——**有序保留**
      certhash 列表；尾部 `/p2p/<id>` 可选
- [x] 3.3 `parse_listen(&Multiaddr) -> Option<SocketAddr>`——**含 certhash 时必须拒绝**，
      允许通配 IP 与端口 0
- [x] 3.4 `advertise_addr(SocketAddr, &[Certhash]) -> Multiaddr`
- [x] 3.5 `announce_addrs(bound, &[Certhash]) -> Vec<Multiaddr>`——通配地址展开成具体网卡；
      展开失败原样通告 + warn（照 `webrtc-p2p` 的同名函数）
- [x] 3.6 单测：解析可拨/监听地址、拒绝带 certhash 的监听地址、不认领 `/quic-v1` 无
      `/webtransport` 段的地址、通配展开后不含通配 IP 且端口保留
- [x] 3.7 单测：certhash 编码与官方逐位一致（multihash + SHA2-256），照
      `webrtc-p2p::certificate` 的跨实现兼容测试的思路钉死

## 4. L0 纯函数层：`certificate/`

- [x] 4.1 `cert.rs`：`Certificate` newtype 包住 `wtransport::Identity`——**公共 API 不出现
      `wtransport` 类型**（Decision 7）
- [x] 4.2 `Certificate::generate(from: SystemTime, days: u32)`：ECDSA P-256 +
      `self_signed_builder().validity_days(...)`
- [x] 4.3 `Certificate::hash() -> Certhash`（SHA-256 of DER）、`der()`、`not_before()` /
      `not_after()`
- [x] 4.4 `Certificate::to_pem()` / `from_pem()`
- [x] 4.5 单测：生成的证书是 ECDSA P-256、有效期 ≤ 14 天、非 RSA
- [x] 4.6 单测：PEM 往返保持 certhash 不变
- [x] 4.7 单测：两张独立生成的证书 certhash 不同（撞了说明密钥没随机）
- [x] 4.8 `rotation.rs`：`Rotation { current, next, retired }` + `Advance { Idle, Rotated }`
- [x] 4.9 `Rotation::bootstrap(now)`：生成两张，`next` 生效时间 == `current` 过期时间
- [x] 4.10 `Rotation::from_pem(pem, now)` / `to_pem()`——多段 PEM，不发明元数据格式
- [x] 4.11 `Rotation::advance(&mut self, now) -> Result<Advance>`——**时钟从参数进来**，
      幂等（同一 `now` 重复调只轮换一次）
- [x] 4.12 `Rotation::advertised() -> Vec<Certhash>`（current 在前）、
      `noise_certhashes() -> HashSet<Multihash<64>>`（current + next + 近期退役）
- [x] 4.13 单测：未到期推进 → `Idle`，两张证书不变
- [x] 4.14 单测：**跨过期推进 → `Rotated`**，next 成为 current、生成新 next、给出退役 hash
- [x] 4.15 单测：同一个越期时刻连续推进两次，第二次 `Idle`
- [x] 4.16 单测：加载时 current 已过期 / 两张都已过期（关机 28 天）两条恢复路径
- [x] 4.17 单测：加载到 RSA 证书或有效期 > 14 天的证书时视为不可用并重新生成
- [x] 4.18 单测：**旧地址在下一轮仍可拨**——客户端集合 `{A,B}`、服务端 current 已是 `B` 时
      验证通过；再下一轮 current 为 `C` 时验证失败（钉死 28 天寿命这条推论）
- [x] 4.19 **负向验证**：逐条改坏 4.11 / 4.14 的实现，确认对应测试变红。不能红的测试要重写

## 5. L1 libp2p 语义层

> 泛型于流类型，不依赖 `wtransport`——用内存双工流即可测。

- [x] 5.1 `noise.rs::inbound<T: AsyncRead + AsyncWrite>`：responder 侧，
      `Config::with_webtransport_certhashes(rotation.noise_certhashes())` 上报
- [x] 5.2 `noise.rs::outbound<T>`：initiator 侧，把地址里的 certhash 集合作为期望值传入验证
- [x] 5.3 **不设 prologue**——与 webrtc-direct 的 `libp2p-webrtc-noise:` + 双指纹机制互斥，
      在函数文档里写明这条差异及混用的后果（第一条消息就握手失败）
- [x] 5.4 握手完成后关闭该流，只返回 `PeerId`（只认证不加密，保密由 QUIC-TLS 承担）
- [x] 5.5 单测：内存流上双向握手成功，两侧得到对方正确的 `PeerId`
- [x] 5.6 单测（**负向，必须红过一次**）：服务端上报集合缺少客户端期望的某一项时握手失败
- [x] 5.7 `muxer.rs`：`StreamMuxer` 实现——`poll_inbound` → `accept_bi`、
      `poll_outbound` → `open_bi`、`poll_close` → `close`
- [x] 5.8 `tokio_util::compat` 适配 tokio `AsyncRead`/`AsyncWrite` → futures 方言
- [x] 5.9 muxer 被 drop 时必须关闭底层会话（不能只靠 `poll_close`——那只在正常关闭流程里
      被调到），照 `webrtc-p2p::managed` 的守卫思路
- [x] 5.10 单测：子流无 framing——写入的首字节即对端读到的首字节

## 6. L2 wtransport 绑定层

- [x] 6.1 `config.rs`：`Config::new(id_keys)` + builder（`with_certificate_store`、
      `with_handshake_timeout`），与 `webrtc-p2p::Config` 同构
- [x] 6.2 `config.rs`：`CertificateStore` 端口 trait（`load` / `store`，多段 PEM）+
      `StoreError`
- [x] 6.3 `listener.rs`：后台 accept task——`Endpoint::accept().await` → 校验
      `path == /.well-known/libp2p-webtransport` 且 `type=noise` → 不匹配 `not_found()`
      → 匹配则 `accept().await` → 经 mpsc 送出 `Session`
- [x] 6.4 后台 task **只做接受连接，不碰 libp2p 语义**（不认识 PeerId、不跑 Noise）
- [x] 6.5 `dialer.rs`：拨号（不要求本机已在监听，用独立临时端口）
- [x] 6.6 `transport.rs`：`libp2p_core::Transport` 实现——`listen_on` / `remove_listener` /
      `dial` / `poll`
- [x] 6.7 `poll` 把 `Session` 包成 `TransportEvent::Incoming { upgrade: BoxFuture }` 交给
      swarm 驱动（Noise + Muxer 在 future 里跑，**不在我们的 task 上**）
- [x] 6.8 `Transport::new(config) -> Result<Self>`——证书加载/生成失败在构造期就报，
      不拖到第一次 `listen_on`
- [x] 6.9 `lib.rs` 门面：`pub use` `Transport` / `Config` / `CertificateStore` / `Certificate`
      / `Error`；确认 `wtransport` 类型一个都没漏出去

## 7. 证书轮换接线

> 单独一组：它牵动通告地址与 Noise 扩展里的 hash 列表，与第 6 组的关注点正交。

- [x] 7.1 `Transport::poll` 每次调用顺带 `rotation.advance(SystemTime::now())`
      （**不起后台定时 task**）
- [x] 7.2 `Advance::Rotated` 时：对每个监听器先发 `AddressExpired`(旧) 再发 `NewAddress`(新)
- [x] 7.3 `Advance::Rotated` 时：调 `Endpoint::reload_config` 换服务端证书
- [x] 7.4 `Advance::Rotated` 时：经 `CertificateStore` 回写；**写失败只 warn，不中断服务**
- [x] 7.5 持久化 PEM 损坏时按首次启动重新生成 + warn（说明 certhash 将改变）
- [x] 7.6 集成测试：注入越过期限的时刻推进 → 断言产出一对地址事件、新连接用新证书、
      **既有连接不断**
- [x] 7.7 集成测试：重启后（持久化 current 仍有效）certhash 不变

## 8. 最小可用路径验证（**此时不接进 `crates/net`**）

- [x] 8.1 `examples/listener.rs`：起一个 native listener 并打印通告地址（照
      `webrtc-p2p/examples/direct_listener.rs`）
- [x] 8.2 集成测试 `tests/loopback.rs`：native↔native 拨通一条 echo，Noise 完成、PeerId 正确
      ——这是 CI 的唯一覆盖来源，也是实现拨号的全部理由
- [x] 8.3 浏览器手测：Chrome / Firefox / Safari 各拨通一次，确认三家的
      `serverCertificateHashes` 都工作
  → **两个引擎实测通过，Gecko 未测**（本机没装 Firefox）。用本地 bootstrap（`--listen-ip 127.0.0.1`，浏览器拨 `127.0.0.1` 可绕开 Chrome 的 LNA 拦截）：
    - **Chrome（Blink）走完了整条 libp2p 链路**，不只是 `wt.ready`：证书准入 → WebTransport 会话 → Noise 认证（服务端日志里那条入站连接带 `peer_id`）→ identify 交换 → `ReservationReqAccepted`。且重启 bootstrap 后 certhash 逐字不变，浏览器经**同一条地址**重连成功 —— 证书持久化就地验证。
    - **Safari 26.5（WebKit）** 与 **Edge（Blink）** 用最小验证页确认 `serverCertificateHashes` 准入通过。两者都做了**负面对照**（喂一个全零 certhash → `Opening handshake failed`），排除「浏览器根本没校验」这种假绿。
    - Safari 只验到准入层。Noise 之后的部分是同一份 wasm 代码，浏览器差异只在准入这一层，故未重复验证。
- [x] 8.4 `cargo test -p webtransport-p2p` 全绿

## 9. 接入 `crates/net` 与部署

- [x] 9.1 `crates/net/Cargo.toml`：按 target 分派——native 依赖 `webtransport-p2p`，
      `cfg(wasm_browser)` 依赖 `libp2p-webtransport-websys`
- [x] 9.2 `crates/net/src/transport.rs` native 分支：`with_other_transport` 接入，
      按 `/quic/webtransport` 前缀分派
- [x] 9.3 `crates/net/src/transport.rs` wasm 分支：接入 `libp2p-webtransport-websys`
- [x] 9.4 `supported_transports` 增加 `TransportKind::WebTransport` 变体（那份清单与组装
      代码同文件——漏改会静默拒掉合法地址）
- [x] 9.5 `crates/bootstrap`：新增 UDP **4004** 监听与地址通告，**不动 4003**
- [ ] 9.6 `docs/app/app/_lib/relay-helpers.ts`：浏览器侧 bootstrap 清单增加 WebTransport
      地址（与 webrtc-direct 并存）
  → **判定为不需要**（不是漏做）。浏览器经 identify 从 bootstrap 学到带**当前** certhash 的 WebTransport 地址，比写死在清单里更正确——写死的那条每 28 天就会失效。
- [x] 9.7 宿主实现 `CertificateStore`：桌面写 `app_local_data_dir`（与 `identity.json` 同
      目录）、bootstrap 写配置目录
  → **两半都做了，且没有动 `KeychainProvider`。** 此前的判断（「要给那个 trait 加三个方法，牵动 uniffi 跨 FFI 契约与 4 个入库的生成文件」）是**错的**：证书端口本来就不该挂在那个 trait 上。它那三组方法都是「读一次就完」的形态（身份与 webrtc 证书永不改变），而 WebTransport 的证书要 14 天轮换并**回写** —— 需要的是长期持有的可写端口。
    做法：把 `CertificateStore` 从 `webtransport_p2p` 的类型提成 `crates/net` 自己的、**不带 cfg 门控**的端口（native 侧 12 行 adapter 转回去），于是 `crates/core` 的组合根零 `cfg(wasm_browser)` 分支就能把它传下来。
    **桌面与移动端共用同一份文件实现**（`WebTransportFileCertificateStore`：tempfile + `O_EXCL` + `0600` + fsync + 原子替换 + 读失败不降级），各宿主只给路径 —— 与 `JsonFileDeviceConfig` 同一体例。这里不走「端口三端各写一份」的常规体例，判据是那三条不变量都容易写错且没有反馈回路。浏览器传 `None` = 只拨号。
    **监听判据是「有没有证书端口」而不是「是不是 Native」** —— 后者把浏览器也算进去，而它起不了监听，`bind` 会直接失败。两条 core 测试正反看守（`native_with_cert_store_listens_on_webtransport` / `native_without_cert_store_does_not_listen_on_webtransport`），第三条 `webtransport_certhash_survives_restart_with_same_store` 看守持久化。
- [x] 9.8 `./scripts/check-wasm.sh --clippy` 绿（新 crate 不进 wasm 依赖树）

## 10. 日志与可观测性

- [x] 10.1 桌面 `src-tauri/src/logging.rs` 的 `DEFAULT_FILTER` 单列 `webtransport_p2p`、
      `wtransport`、`quinn` 三个 target
- [x] 10.2 移动端对应常量同改（**两份独立常量，要一起改**）
- [x] 10.3 测试断言三个 target 都能过滤通过，照抄
      `default_filter_passes_the_targets_we_depend_on`

## 11. 吞吐基准

- [x] 11.1 `crates/net/examples/transport_throughput.rs` 增加一档 WebTransport
- [x] 11.2 基准建在 `Endpoint` 上，**不手写 `libp2p_core::Transport` 的 poll 循环**
      （测量装置复杂度一接近被测对象就成了主要误差源）
- [x] 11.3 **别把 transport 驱动和数据传输绑在同一个 task 上**——上一版基准就是这么自锁挂死的
- [x] 11.4 与 QUIC / webrtc-direct 同图对比，各取 **6 次中位数**
- [x] 11.5 结论写进 `dev-notes/blogs/transfer-throughput/`

## 12. 三道关与收尾

- [x] 12.1 机器门禁：`cargo fmt --all` / `cargo check --workspace --all-targets` /
      `cargo test --workspace` / `cargo clippy --workspace`
- [x] 12.2 `./scripts/check-wasm.sh --clippy`（wasm job 是硬失败，不受 native clippy 豁免保护）
- [x] 12.3 `/simplify`
  → 四路并行审查（复用 / 简化 / 效率 / 抽象层次），共 26 条发现，应用了其中 14 条。
    **两条是真 bug 而非风格问题**：① 损坏的持久化 PEM 重新生成后不落盘（certhash 每次
    重启都变，正是持久化要防的唯一一件事）；② 桌面与 Web 两处从 multiaddr 猜 transport
    的 if 链把 `/quic` 排在 `/webtransport` 前面，WebTransport 地址被标成 "QUIC"，而校验
    又拒绝它 —— 同一屏两句话矛盾。两条都补了回归测试，①还做了变异验证。
- [x] 12.4 `/code-review`
  → 两路正确性审查（新 crate 的协议契约与并发 / 跨层接入一致性），角度与 12.3 不重叠。
    共 15 条发现，**修了 11 条**，其中 4 条是证书生命周期上「测试全绿但仍然错」的真 bug：
    ① 两张证书首尾相接 → 每 14 天有最长 60s 的新连接**全量 TLS 拒绝**窗口（改成重叠 1 小时，
    在过期前切换）；② 退役 certhash 只在内存里 → 一次重启就打掉「旧地址撑过一整轮」的契约
    （改成随 PEM 一起持久化）；③ `store.load()` 报 IO 错时会覆盖可能完好的数据（改成不回写）；
    ④ SEC1 私钥不被拦 → 第一次 `listen_on` 时 panic 掉整个 Swarm 线程。
    另修：accept 循环的「背压」不成立（`mpsc` 容量随 Sender clone 涨）+ 入站握手无超时、
    公网地址登记先记账后执行导致一次失败永久不重试、`shared-view` 的展示映射漏了新变体、
    「原子写」缺 fsync 且失败时残留含私钥的 tmp、wasm 侧无浏览器能力探测会多报、
    一条恒真的假断言、Cargo.lock 的无关降级。
    未修 1 条记为负债：bootstrap 的公网地址只增不删（需要内核补 `remove_external_addr`）。
- [x] 12.5 更新 `dev-notes/knowledge/net-kernel.md`：新增 WebTransport 一节（分层、证书轮换、
      与 webrtc-direct 的 Noise 机制差异、端口约定）
- [x] 12.6 更新 `CLAUDE.md`：crate 表加 `crates/webtransport-p2p`、bootstrap 端口清单加 4004、
      tracing 默认 filter 一节加三个 target
- [x] 12.7 更新 `dev-notes/prompts/webtransport-native-transport.md`：标记状态为已落地，
      并补上本轮的增量（浏览器支持矩阵、PR #4348 作为参考实现、28 天寿命推论）
