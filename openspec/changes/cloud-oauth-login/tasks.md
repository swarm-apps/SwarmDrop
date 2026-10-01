# Tasks

## 1. crate 骨架与依赖

- [x] 1.1 新建 `crates/cloud-auth`（`swarmdrop-cloud-auth`），加入根 `Cargo.toml`
  workspace 成员与依赖声明（tokio / serde / serde_json / thiserror / tracing /
  async-trait / oauth2 / reqwest rustls-tls），`src-tauri` 声明依赖；验证
  `cargo check --workspace --all-targets` 通过
- [x] 1.2 定义端口与核心类型：`CloudAccountProvider` trait、`ProviderId`、
  `ConnectSession`（含交互指引）、`AccountSnapshot`、`AccessTokenLease`、错误类型
  （thiserror，失败类别对齐规格「凭证失效」「保存失败」「被拒绝」等）；编译通过
  即验证
- [ ] 1.3 记录二进制体积基线：合入前对桌面 release 构建量一次增量（reqwest 依赖
  树），写进变更说明

## 2. 凭证存储（对应 cloud-credential-storage 规格）

- [x] 2.1 实现 `cloud-accounts/<provider>/<account-id>.json` 的文件存取：照
  `crates/host-fs/src/identity_store_file.rs` 的 tempfile + O_EXCL + 0600 + rename
  体例；单测覆盖权限断言与「写入中断后为完整旧版或新版」
- [x] 2.2 单测：账户 A 轮换覆写不触碰账户 B 的凭证文件（内容与 mtime 均不变）；
  删除后凭证与临时文件均不存在
- [x] 2.3 单测：凭证序列化不进入 tracing 输出（刷新失败路径的日志断言无 token
  字段值）

## 3. Google provider 与授权流（对应 cloud-account-auth 规格）

- [x] 3.1 实现 loopback 回调监听：`tokio::net::TcpListener` 绑 127.0.0.1 临时端口、
  一次性随机 state、单次请求应答后关闭；单测覆盖 state 不一致拒绝与超时清理
- [x] 3.2 实现授权码交换与刷新（oauth2 crate，`extra_params` 带
  `access_type=offline` + `prompt=consent`）；单测用假 token 端点覆盖交换、刷新、
  刷新失败三类路径
- [x] 3.3 实现轮换顺序「先落盘成功再暴露新凭证」+ 每账户刷新串行化（D6）；单测照
  storage-abstraction.md 桩法（可撑开窗口 + 进闸回执，multi_thread flavor）钉
  「窗口内不发生第二次刷新」与「落盘失败停用账户」；增加模拟「远端已轮换、
  落盘前退出 → 重启旧值遭拒 → 需要重连」的故障路径
- [x] 3.4 实现断开连接：尽力调 provider 撤销端点、失败仍删本地凭证并如实反馈；
  单测覆盖撤销成功 / 撤销失败两条路径

## 4. 账户管理器与 IPC（对应 cloud-account-auth 规格）

- [x] 4.1 实现 `CloudAccountManager`：多账户并存、列表 / 状态查询（正常 / 需要重连 /
  刷新中）、`access_token(account, min_validity)` 供进程内存储层取短期凭证；单测
  覆盖多账户隔离、并发请求只刷新一次、token 过期与状态迁移
- [x] 4.2 新增 `src-tauri/src/commands/cloud_account.rs` 命令组（connect 立即返回
  授权 URL、disconnect、list、status），`setup.rs` 组合根建一次
  `Arc<CloudAccountManager>` 并 `app.manage`；验证 IPC 返回值不含任何凭证字段
- [ ] 4.3 连接完成 / 失败 / 超时与账户状态变化经 Tauri 事件推送；用
  `pnpm tauri dev` 手动验证事件到达前端

## 5. 设置页 UI 与文案

- [x] 5.1 设置页新增「云存储账户」分区：BYO client 表单（client_id / secret 必填
  校验）、连接按钮（经 external_open 打开浏览器）、账户列表（状态 + 断开）、
  「需要重新连接」引导态；组件测试覆盖必填校验与状态渲染
- [x] 5.2 Lingui 三语（zh / en / zh-TW）文案补齐，`pnpm exec tsc --noEmit` 通过
- [x] 5.3 用户文档：Google Cloud Console 申请步骤（Desktop 类型 client、Test
  users、Testing 7 天限制与 Push to production 路径、国内可达性需代理）写入
  `docs/` 或 README 相应章节；说明 provider 到期或轮换落盘窗口仍可能要求重连

## 6. 集成验收

- [ ] 6.1 端到端手验：真实 BYO client 走完「连接 → 重启仍已连接 → 断开」旅程，
  截图或录屏存变更记录；凭证文件权限与位置复核（0600、本机数据目录）
- [x] 6.2 门禁全绿：`cargo fmt --all`、`cargo check --workspace --all-targets`、
  `./scripts/check-wasm.sh`（确认六 crate 门禁不受新 crate 影响）、
  `pnpm exec tsc --noEmit`
- [x] 6.3 与 `storage-cloud-gdrive` 对账：存储 adapter 只能获取短期 access token；
  不传 refresh token / client_secret 给任何存储 SDK，401 由账户管理器刷新后至多重试一次
