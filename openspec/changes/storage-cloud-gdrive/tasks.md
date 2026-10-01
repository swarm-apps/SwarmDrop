# Tasks

## 1. 契约探针与依赖边界

- [ ] 1.1 用真实 BYO Google OAuth client 验证 `drive.file` 对应用创建目录、`appProperties` 写入/检索、文件 ID 查询和浏览器打开的权限；记录 client 更换后私有属性不可见的行为。若范围不足，先修订 scope 决策与授权文案
- [ ] 1.2 用 Drive API 探针验证 resumable session 的初始化、按服务端偏移恢复、完成响应丢失对账与 session 过期重建；记录非末块粒度和失败码
- [x] 1.3 在 `crates/storage-cloud` 建 native-only crate 与 `GoogleDrivePublisher` 骨架；依赖统一声明在 workspace，确认 `./scripts/check-wasm.sh` 不引入 native HTTP 依赖

## 2. 云发布端口与 Drive 实现

- [x] 2.1 实现窄端口 `CloudPublisher`、`StagedFile`、`PublishIntent`、`CloudObjectRef`；provider 私有上传会话与 HTTP 类型不进入 host/transfer/IPC
- [x] 2.2 从账户管理器逐次领取 `AccessTokenLease`；上传分块间可更新 token；401 最多经账户管理器刷新并重试一次，adapter 不接触 refresh token / client_secret
- [x] 2.3 实现 Drive 目录创建、路径清理、冲突后缀及 `appProperties` 溯源字段；使用完整稳定接收设备 ID 分区，按接收记录键查询并核对账户、长度、根哈希和位置
- [x] 2.4 实现有界分块上传与背压、进度回调、限速退避和错误归一；记录缓冲与并发上界，错误和日志不包含 token、session URI 或签名 URL
- [x] 2.5 实现本机 0600 原子检查点：暂存身份、session URI、服务端确认偏移及创建时间；重启时查询远端进度并核对暂存，session 过期时仅重启云上传
- [x] 2.6 完成响应丢失时先按接收记录键或已持久化对象 ID 对账；同一账户、接收设备、目标路径和根哈希重复接收可复用对象，同时为本次接收保留独立记录

## 3. 接收编排与持久化

- [x] 3.1 扩展中立 `CoreSaveLocation` / `FinalizedSink` / 文件位置类型，支持本地和云端；host/transfer/entity 只含目的地与对象身份，不依赖 Drive API、OpenDAL 或 HTTP 客户端
- [x] 3.2 给云暂存增加稳定 session_id / file_id 身份；保留 host-fs 随机写、bao 校验和既有 P2P 位图，单文件收齐后从暂存触发云发布
- [ ] 3.3 实现「远端完成 → 对象位置和完成位图入库 → 暂存及上传检查点清理」顺序；发布失败保留暂存，清理失败由重启对账处理；覆盖每个崩溃窗口
- [ ] 3.4 扩展 storage-sql 的会话与收件箱位置字段并迁移旧本地路径；旧条目打开、导出和删除行为保持有效，云条目不走本地文件操作端口
- [x] 3.5 定义云阶段的取消、主动放弃和重试语义；取消不把未完成云文件记为 completed，暂存删除须与用户选择一致

## 4. 桌面 UI 与用户说明

- [x] 4.1 接收确认页以目的地选择器替代仅选本地目录的入口；仅展示可用账户，无账户时提供设置入口，选择结果传到接收编排
- [x] 4.2 分别展示 P2P 接收和云上传进度及速率；云上传完成前整体仍在进行中，错误呈现可重连、可重试、配置错误等操作入口
- [x] 4.3 收件箱和历史使用显式云位置：账户、路径、浏览器打开；隐藏本地专属动作，删除记录时说明远端文件仍保留，断开账户后保留历史
- [x] 4.4 在选择与结果页说明传输段 E2E、落云明文及云 ACL；文案进入 Lingui 三语

## 5. 验收与文档

- [ ] 5.1 用真实 Google 账户验收大文件、有界内存、断网与进程重启恢复、session 过期、完成响应丢失、同一目的地复用、两台设备各自可见、旧数据库迁移
- [ ] 5.2 记录代表性文件的 P2P 和云上传速率、内存与暂存磁盘峰值；与 issue #132 的吞吐基线对照，不能仅凭单次上传成功判定恢复能力
- [x] 5.3 更新 `CLAUDE.md` 模块边界和用户文档中的 BYO、暂存磁盘需求、恢复与 E2E 边界；记录 Drive API 探针及未来阿里云盘能力清单
- [x] 5.4 完成仓库门禁：`cargo fmt --all`、`cargo check --workspace --all-targets`、`./scripts/check-wasm.sh`、`pnpm exec tsc --noEmit`

## 2026-10-01 实施记录

- 核心实现与桌面 UI 已落地；原生/wasm 编译、桌面/移动/文档站类型检查通过。
- 真正完成对象发布之后才写完成位置；新增 `staged_complete` 记录已同步的完整暂存，恢复时可只重试云发布。
- 当前上传取消沿用接收 actor 的发布收敛规则：等当前文件发布与记账结束后取消会话；明确保留已发布远端文件。
- 1.1 / 1.2 只验证了当前客户端、17 MiB 上传、私有属性查询及复用，第二客户端、浏览器实际打开和恢复故障场景未验收。
- 3.3 / 3.4 实现已完成，但崩溃窗口与存量数据库实际打开/导出/删除未验收，保持未勾选。
- 5.1 / 5.2 仍需真实设备与故障注入，当前探针不能替代。结果和边界见 `docs/cloud-storage.md`。
