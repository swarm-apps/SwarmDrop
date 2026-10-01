# Design

## Context

动机与调研证据见 proposal「Why」。代码侧的现状约束：

- **端口-适配器纪律**（`dev-notes/knowledge/storage-abstraction.md`）：端口 trait
  wasm-clean、native 实现独立 crate（`swarmdrop-storage-sql` 先例：不依赖 core、
  只吃端口与数据类型、宿主在组合根注入）。云账户层没有 wasm 需求，但同一纪律适用：
  新 crate 不进 `crates/core`。
- **0600 原子写体例**：`crates/host-fs/src/identity_store_file.rs` 已有成熟的
  「tempfile 随机名 + O_EXCL + mode 0600 + rename」实现与测试，凭证存储照抄这套
  体例而不是再发明。
- **打开外部 URL**：`src-tauri/src/commands/external_open.rs` 已存在，浏览器拉起
  复用它。
- **桌面身份不走系统钥匙串**（`desktop-identity-storage` 规格）：云凭证同判据——
  回环监听与 token 交换都在应用内完成，没有理由引入钥匙串交互；保持文件形态，
  debug / release 一致。

## Goals / Non-Goals

**Goals**

- 端口只表达授权会话、账户状态和短期 access token；各 provider 自己处理授权参数与刷新响应。
- 轮换语义从第一天就在持久化路径上，并如实呈现远端轮换与本地落盘之间的崩溃窗口。
- 桌面端一个可独立验收的用户旅程：填 BYO client → 浏览器授权 → 已连接 → 断开。

**Non-Goals**

- 云存储操作（`storage-cloud-gdrive` 变更）。
- 阿里云盘扫码登录的 UI（端口形状容纳「非浏览器回调的交互」，实现后置）。
- 移动端（深链回调 + Keychain 变体，`opendal-mobile-feasibility` 已评估后置）。
- token 的内存加密（静态防护 = 文件权限 0600，与设备私钥同水位）。

## Decisions

### D1：抽象「token 生命周期 provider」，不抽象「通用登录」

调研结论（2026-10-01）：三家目标 provider 全是 OAuth 2.0，通用登录层没有第二个
实例去支撑；真正的分叉是 token 生命周期（Google 通常不轮换 refresh token，
阿里云盘每次刷新轮换）。因此 `CloudAccountProvider` 端口的方法面围绕凭证生命周期组织：

```
provider_id() -> ProviderId
start_connect(client 凭证, 账户标签) -> ConnectSession   // 携带交互指引（授权 URL / 未来：QR）
poll/complete(ConnectSession) -> AccountSnapshot          // 一次性等待完成
access_token(account, min_validity) -> AccessTokenLease    // 必要时串行刷新；只返回 access token 与过期时间
revoke(account) -> RevokeOutcome
```

被否决的备选：①「通用登录层」（login with X）——只有一种登录方式，抽象无第二实例；
②直接把 refresh token 塞给存储 adapter 或 OpenDAL builder——阿里云盘轮换 +
OpenDAL 不回吐 = 重启即失效（见 proposal「Why」）。存储层按请求领取 access token；
刷新、持久化和重连状态只有账户管理器能够决定。

### D2：crate 形态——`crates/cloud-auth`，native-only，桌面声明

照 `swarmdrop-storage-sql` 先例：不依赖 `swarmdrop-core`；依赖 `swarmdrop-host`
（错误/数据类型如适用）+ tokio + serde。桌面 `src-tauri` 声明依赖；移动端将来在
`mobile-core` 加依赖（深链回调变体实现同一个 trait）。不进 check-wasm 六 crate
名单，`crates/core` 零改动。

被否决的备选：把端口放进 `crates/host/src/ports.rs`——那里是 core 反向依赖的宿主
端口（FileAccess / KeychainProvider），云账户是纯宿主侧能力，core 不消费它；放过去
会扩大 core 的概念面。

### D3：HTTP 与授权流——`oauth2` crate 用于标准 provider，轮换 provider 走手写薄层

- Google：`oauth2` crate（ramosbugs/oauth2-rs）覆盖授权码 + PKCE + state + token
  交换；Google 的非标参数（`access_type=offline`、`prompt=consent`）经
  `extra_params` 传入。loopback 回调不起 axum——`tokio::net::TcpListener` 手写
  最小 HTTP 应答（一次性 200/重定向页 + 关闭），避免为一个端点引入服务器框架。
- 阿里云盘（预埋）：其 token 端点收 JSON body 而非 RFC 6749 form 编码，`oauth2`
  crate 的标准请求构造用不上；届时在 provider 实现内手写这两个 JSON POST（刷新
  本来就要自己管轮换）。端口不受影响。
- HTTP client：新增 `reqwest`（rustls-tls，workspace 统一声明），不复用
  `tauri-plugin-http`——后者是前端插件语义，crate 层不该依赖 Tauri 生态。

### D4：凭证存储——host-fs 体例的 0600 原子写，按账户一文件

目录：应用本机数据目录下 `cloud-accounts/<provider>/<account-id>.json`（与设备私钥
同根不同子目录）。写入照 `identity_store_file.rs` 的 tempfile + rename 体例。文件
内容（serde JSON）：client_id / client_secret / refresh_token / access_token /
expires_at / 状态等。轮换刷新成功后的顺序是「收到 provider 响应 → 写盘成功 → 更新
内存视图并发放新 access token」。写盘失败时当前进程保留新 token 供受控重试落盘，
但暂停该账户的云操作；进程在响应与落盘之间崩溃，远端已作废的旧 token 无法恢复，
重启后的 `invalid_grant` 必须转为「需要重新连接」。这不是本地原子写能保证的事务。

被否决的备选：SQLite（storage-sql）——凭证是按账户整体覆写的 blob，无查询需求，
进数据库反而把「删除即清除」变成软删语义；钥匙串——与桌面身份的零交互判据冲突
（桌面纪律明确不走系统安全存储）。

### D5：IPC 面——命令组 + 事件，前端只见状态

`src-tauri/src/commands/cloud_account.rs`：`list_cloud_accounts` /
`connect_cloud_account(client_id, client_secret)` / `disconnect_cloud_account(id)` /
`cloud_account_status(id)`。连接是长交互：命令立即返回会话句柄 + 授权 URL（经
`external_open` 打开浏览器），完成/失败/超时经事件推送（`cloud-account-updated`）。
组合根（`setup.rs`）建一次 `Arc<CloudAccountManager>` 并 `app.manage`，命令与事件
同源——「注入的与自持的是同一个 Arc」的组合根纪律。`AccessTokenLease` 只在 Rust
进程内传给存储 adapter，绝不作为 IPC 返回值或事件载荷。

### D6：轮换 provider 的并发纪律

同一账户的刷新串行化（`Mutex<()>` per account）：拿到锁后重新检查 token 的剩余
有效期，避免并发调用重复刷新。锁覆盖 provider 响应、原子落盘和内存视图更新；
新 access token 只在落盘成功后发放。测试照 storage-abstraction.md 的桩法
（可撑开的窗口 + 进闸回执）钉住「窗口内并发刷新不发生」。存储 adapter 遇到
401 时只请求账户管理器刷新并重试一次，不能各自发起 refresh 请求。

## Risks / Trade-offs

- [Google 同意屏停在 Testing 时 refresh_token 可能 7 天过期] → 状态机覆盖「需要
  重新连接」，规格不承诺永久免重连；用户文档写明发布配置的影响。
- [Drive scope] → 首期只操作应用创建的 SwarmDrop 目录，优先验证 `drive.file`；
  若真实探针证明不足，再记录需要 `drive` 的操作与理由，调整授权与文档。scope
  是 Google provider 的显式配置，不由 OpenDAL 决定。
- [loopback 监听被本机其他进程抢占/伪造] → 临时端口 + 一次性随机 state 校验；
  回调只接受单次请求即关闭监听。
- [新增 reqwest 依赖树体积] → 桌面端已有等价 HTTP 栈（tauri-plugin-http 底下同为
  reqwest），实际增量集中在 native target；记录于 tasks，合入前量一次二进制增量。
- [阿里云盘接入时发现端口形状不够] → 三个已知差异（JSON body、扫码交互、轮换）
  都已在端口形状里留了位；若仍不够，crate 内演化 trait 是单宿主变更，不碰 core。

## Migration Plan

纯新增能力，无存量数据迁移。回滚 = 移除 crate 与命令组，不影响既有功能；凭证目录
不存在时账户列表为空，是合法初态。

## Open Questions

- 设置页「云存储账户」分区的具体路由与信息架构（现有设置页结构落地时对齐）。
- 账户显示标识取什么（Google 的 userinfo 端点要额外 scope/请求；首期可先用
  「Google 账户 #n」占位，连接成功事件里带回 provider 侧可得的标识）——不影响
  端口与存储形状。

### D7：按账户与授权职责组织模块（2026-10-01）

账户视图住 `account`；秘密、访问租约住 `credentials`；提供方标识与授权契约住 `provider`；
生命周期住 `manager`；Google 的回环 HTTP 解析住私有 `google/loopback`；持久化住 `store`。
各模块显式导出所需接口，不使用类型汇总文件或通配导出。默认账户标签由提供方标识描述，
仓库按持久化目录枚举账户，注册提供方集合只由管理器维护。

刷新后的 token 再次被服务端拒绝时，管理器核对当前 token 代次后广播并持久化重连状态；
旧请求不会把随后完成的新授权降级为失效。
