# Proposal

## Why

issue #132 把「OAuth 基建」列为 Google Drive / OneDrive / 阿里云盘的前置项。
无论存储侧使用厂商 API 还是 OpenDAL，授权、回调、token 交换与持久化都由应用负责。
存储侧只能领取有期限的 access token，不得自行持有 refresh token。

2026-10-01 的调研结论进一步收紧了设计约束：后续排期的阿里云盘同样是标准 OAuth 2.0
（授权码 + 刷新令牌），**不需要**为「非 OAuth 登录方式」抽象通用登录层；但它的
refresh_token **每次刷新轮换、旧的立即失效**，且 OpenDAL 的 aliyun-drive service 把轮换
结果只写在内存里、不回吐给调用方——意味着 token 生命周期必须由本应用持有并持久化，
不能委托给该 service。Google Drive 首期直接调用 Drive API；阿里云盘接入时，
即使用 OpenDAL 处理部分文件操作，也只传入应用管理的短期 access token。

## What Changes

- 新增 `swarmdrop-cloud-auth` crate（`crates/cloud-auth`，native-only，桌面宿主声明
  依赖，照 `swarmdrop-storage-sql` 先例；不进 `crates/core`，wasm 门禁不受影响）：
  - `CloudAccountProvider` 端口（trait）：发起授权、等待完成、刷新、获取有效期内的
    access token、断开连接。每 provider 一个实现，首期只实现 Google；刷新并发按账户
    串行化，存储 adapter 只读取 access token，不读取 client_secret / refresh_token。
  - Token store：refresh_token / access_token 按账户落盘，0600 + 原子替换写，与设备
    私钥同级同纪律；每次轮换返回的新 refresh_token **落盘成功后才能对其他操作可见**。
    provider 已轮换、应用尚未落盘之间的进程崩溃窗口无法用本地原子写消除；旧 token
    失效时进入「需要重新连接」，不得承诺此情形下自动保持登录。
- 桌面端云账户登录流（BYO 凭证原则，#134 限定例外）：用户自填 Google Cloud Console
  申请的 client_id / client_secret → 系统打开浏览器授权（Desktop 类型 client +
  `http://127.0.0.1:{ephemeral}` loopback 回调，RFC 8252；`access_type=offline` +
  `prompt=consent` 强制下发 refresh_token）→ 回调完成 token 交换并落盘。
- 桌面 IPC 与 UI：`cloud_account` 命令组（列表 / 发起连接 / 查询状态 / 断开）、连接
  完成 / 失败事件；设置页新增「云存储账户」分区；zh / en / zh-TW 三语文案。
- 明确不做（后续变更）：云接收与 Drive API 本体（`storage-cloud-gdrive` 变更）、
  OneDrive 与阿里云盘的 provider 实现（仅保证端口形状容纳）、移动端深链回调变体
  （`opendal-mobile-feasibility` 已评估后置）。

## Capabilities

### New Capabilities

- `cloud-account-auth`：云账户授权与生命周期——BYO 凭证（无内置默认 client）、授权
  交互（浏览器 + loopback 回调的防劫持要求）、单点刷新与有期限 access token、
  轮换失败的恢复语义、账户状态查询、断开连接、同 provider 多账户。
- `cloud-credential-storage`：云凭证持久化纪律——与私钥同级的 0600 原子写、单账户
  单文件、轮换覆写的原子性（掉电后要么旧 token 要么新 token，不得半写）、凭证不进
  传输历史 / 日志 / 配对漫游。轮换的本地原子性不等于 provider 与本地的跨系统原子性。
  与 `desktop-identity-storage` 平行的存储纪律规格。

### Modified Capabilities

（无。现有规格中没有云账户 / OAuth 相关能力；`desktop-identity-storage` 管的是设备
身份，边界不重叠。）

## Impact

- **新 crate**：`crates/cloud-auth`（+ 根 `Cargo.toml` workspace 成员与依赖：HTTP
  client、授权流实现；阿里云盘的 JSON body token 端点在其 provider 内处理）。
- **桌面壳**：`src-tauri/src/commands/cloud_account.rs`（新命令组）、连接结果事件、
  `setup.rs` 组合根处建一次 provider registry 并注入；复用 `external_open` 打开浏览器。
- **前端**：设置页「云存储账户」分区（连接向导、状态、断开）；Lingui 三语 catalog。
- **依赖面**：loopback 监听器（tokio TcpListener 最小实现或等价）；不新增 UI 框架
  依赖。
- **不动**：`crates/core`、传输管道、配对体系、wasm 六 crate 门禁。
