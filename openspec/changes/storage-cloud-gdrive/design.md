# Design

## Context

现有接收 actor 把乱序分块写进本地暂存，单文件收齐后调用 `FileAccess::finalize_sink`，
紧接着写 `mark_file_completed`。收件箱把 `FinalizedSink.uri` 存为必填 `local_path`，
桌面打开、显示文件夹、导出和删除均把它当本地文件使用。因此增加云目的地必须改
中立位置模型与发布后确认点，不能只在桌面 UI 加一个选项。

`cloud-oauth-login` 交付进程内 `access_token(account, min_validity)`。存储层不接触
refresh token / client_secret；它只拿到有期限的 access token，在 401 后交由账户
管理器刷新并至多重试一次。实现顺序以该变更为前置。

## Goals / Non-Goals

**Goals**

- 保留已验证的 P2P 随机写暂存和 bao 分块校验；云上传不进入逐个 P2P 块的同步等待。
- 大文件云上传有界内存、可在应用重启后从有效的远端会话继续；远端会话过期时仅
  重启云上传，绝不重传已收齐的 P2P 数据。
- 文件完成、收件箱可见、暂存清理之间有明确提交顺序，完成响应丢失后可对账。
- 云厂商差异留在 native adapter；共享 crate 只持有目的地与对象位置等领域事实。

**Non-Goals**

- send-from-cloud、阿里云盘 / OneDrive / OSS 后端、移动端云上传。
- 跨设备物理去重、云端 watcher、持续同步和后台长期排队上传。
- 一次发布到多个云账户；首期一次接收只选一个目的地。

## Decisions

### D1：以「发布已校验文件」为端口，厂商 API 是实现细节

新增 `crates/storage-cloud`，对桌面宿主暴露窄端口：

```text
CloudPublisher::publish(StagedFile, PublishIntent, ProgressSink) -> CloudObjectRef
CloudPublisher::open_url(CloudObjectRef) -> Url
```

`PublishIntent` 包含账户、接收设备、session_id / file_id、目标相对路径、长度和
BLAKE3 根哈希。`StagedFile` 提供有界区间读取及稳定身份。`publish` 内部负责远端
查重、创建 / 恢复上传会话、完成和对账；provider 私有的 upload ID / session URI
只在 adapter 与检查点存储之间流动，不进公共端口或 IPC。后续需要读源、远端删除时
再增加对应的窄端口，不预先做一个仿 POSIX 的总接口。

首期 `GoogleDrivePublisher` 直接调用 Drive API。OpenDAL 只在某个后端**实际满足**
发布所需的元数据、上传恢复与内存上界时才作为该后端内部实现；未满足时可在同一
crate 内使用厂商 API。阿里云盘未来沿用 `PublishIntent` / `CloudObjectRef`，但其
创建文件、分片、秒传及元数据仍由阿里云盘 adapter 自己决定。OSS 可单独评估
OpenDAL multipart 能否满足检查点恢复。OAuth、HTTP 客户端和 OpenDAL builder 均
不出现在 `crates/core` / `crates/transfer` 的类型签名里。

### D2：账户管理器是唯一 token 生命周期所有者

Drive 的每个 API 请求从 `CloudAccountManager` 领取足够有效期的 access token；
上传多块时可在块间更换 token。401 触发账户管理器单次刷新与请求重试。
不得给 OpenDAL builder 传 refresh token / client_secret：阿里云盘 OpenDAL service
当前只在内存里更新轮换后的 refresh token，重启会丢失它。若后续使用 OpenDAL，
只能传短期 access token，按有界操作重建或刷新操作会话。

### D3：首期两段式；发布后才写完整 checkpoint

桌面接收仍经本机 `host-fs` pwrite 暂存。`HostFileMetadata` 增加中立的
session_id / file_id，云目的地暂存路径由它们与相对路径确定，重启后可重新打开。
单文件收齐后，host 侧发布分派调用 `CloudPublisher::publish`；完成后返回带
`CloudObjectRef` 的 `FinalizedSink`。接收 actor 先 `mark_file_completed` 写完整位图和
远端对象位置，再确认发布并删除本地暂存。发布失败保留暂存和未完成位图。

`finalize_sink` 与 `mark_file_completed` 之间崩溃时，云端可能已完成而本地尚未
记账。重试先按接收记录键查询远端对象，命中并核对长度 / 根哈希后补记账，不能
盲目再创建。暂存清理失败由重启后的对账任务处理，不能把已完成文件标成失败。
这一变更允许修改 `crates/host` 与 `crates/transfer` 的中立端口和 actor 收尾逻辑；
不引入 Drive / OpenDAL 依赖，wasm 门禁仍须通过。

单文件发布会在该文件边界等待云端；同一文件的 P2P 分块不会逐块等待云请求。
首期不承诺多文件接收段与上传段完全并行，进度分别报告 P2P 与云上传速率。

### D4：Drive 原生上传与持久检查点

Google Drive API 的 resumable upload 在初始化后返回 session URI；它是可直接
上传的能力 URL，应按凭证保护。`storage-cloud` 在本机数据目录以 0600、同目录
临时文件 + 原子替换保存检查点：账户 / session_id / file_id、暂存长度与哈希、
session URI、已确认偏移和创建时间。日志与 IPC 不得包含该 URI。

上传只读有界块，Drive 非末块大小按其要求对齐；每块成功后落盘进度。恢复时先
向 Drive 查询服务器实际收到的字节数，以服务端为准，核对暂存文件仍是同一内容。
session 失效时丢弃旧检查点，从本地暂存发起新云上传；P2P 位图保持不变。
完成响应丢失时先按 D5 的接收记录键查询已完成文件；确认存在后直接返回对象 ID。
只有远端完成、对象 ID 入库后才清理暂存与检查点。

### D5：写入隔离与幂等范围

用户 Drive 中由应用创建 `SwarmDrop/<receiver_device_id>/...`。使用稳定完整设备 ID
或其无碰撞编码作目录名；短 ID 只用于 UI，不能独自承担隔离。相对路径在云端做
路径清理与冲突处理：同名不同内容生成可读的冲突后缀，不覆盖旧文件。

Drive `appProperties` 记录 `receipt_key`（接收设备 + session_id + file_id）、
双方设备 ID、原始文件名和 BLAKE3 根哈希。相同 `receipt_key` 的重试以远端查询
消除「完成响应丢失」产生的重复；同一账户、同一设备、同一目标路径且根哈希相同
的再次接收可跳过上传，但须生成本次独立的收件记录并指向同一远端对象。
其他设备各有自己的可见文件，不承诺共用一份物理对象。

`appProperties` 对 OAuth client 私有。用户更换 BYO client 后，无法假定新 client
可查询旧 client 的属性；有本地远端 ID 时先核对它，无法证明同一对象时按路径
冲突处理，绝不以「同名」推断内容相同或静默覆盖。Drive API 的元数据写入、
按属性查询和返回对象 ID 是本期强制探针；失败则不能宣称该后端满足幂等契约。

### D6：中立位置模型与用户动作

`CoreSaveLocation` 增加云目的地变体（只含 provider、账户 ID、根目录选择），
`FinalizedSink` 与持久化文件位置改为 `Local { uri, dir } | Cloud { provider,
account_id, object_id, display_path }`。旧数据库本地 `local_path` / `local_dir` 迁移
到 Local 变体；收件箱与会话共用一个中立解析规则。云条目不再填假本地路径。

首期云条目的「打开」调用 `open_url` 在系统浏览器打开 Drive 文件；本地专属的
「在文件夹中显示」和「从本地导出」入口不对云条目出现。删除收件记录默认保留
远端文件并明示这一点；远端删除属于后续独立操作，不得把云对象 ID 传给本地
`FileAccess::delete_finalized_file`。账户断开后条目仍保留可读的云位置，操作提示
重连。迁移与 UI 需要覆盖旧版数据及混合历史。

### D7：错误、隐私与发布边界

adapter 把 401 / `invalid_grant` 映射为「需要重新连接」，限速 / 网络 / 5xx
映射为可重试，权限与目录问题映射为配置错误，检查点失效进入重建云上传。
错误信息只含账户标识、provider 与操作；不记录 token、session URI 或带签名 URL。
云端完成前会话不能进 completed，也不能生成已完成收件箱条目。UI 在选择云目的地
和结果页说明传输段 E2E、落云为明文、云 ACL 决定落地后的私有性。

## 实现探针与证据

- [OpenDAL GDrive backend](https://opendal.apache.org/docs/rust/src/opendal_service_gdrive/backend.rs.html)
  当前声明 `OneShotWriter`；[writer](https://opendal.apache.org/docs/rust/src/opendal_service_gdrive/writer.rs.html)
  调普通上传，[上传请求](https://opendal.apache.org/docs/rust/src/opendal_service_gdrive/core.rs.html)
  只带名称与父目录。
- [Drive resumable upload](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
  定义 session URI、状态查询、续传与过期重建；[文件属性查询](https://developers.google.com/workspace/drive/api/guides/search-files)
  支持 `appProperties`，但属性对 OAuth client 私有。
- [OpenDAL AliyunDrive signer](https://opendal.apache.org/docs/rust/src/opendal_service_aliyun_drive/core.rs.html)
  在内存里替换 refresh token；[writer](https://opendal.apache.org/docs/rust/src/opendal_service_aliyun_drive/writer.rs.html)
  把 upload ID / part number 留在 writer 内存。阿里云盘阶段必须重新核对其开放平台
  当前接口，不能把 OpenDAL 的「可分片」当作「可跨进程续传」。

## Open Questions

- `drive.file` 是否覆盖应用创建目录、重启后 `appProperties` 检索及浏览器打开链路，
  由真实 BYO 客户端探针决定；不足时另提 scope 变更，不能自动扩大权限。
- Drive 文件在用户手动移动 / 重命名或删除后，旧收件记录的打开行为与缺失状态，
  首期按远端 ID 查询结果显示「已移动 / 不可访问」，不猜测新路径。

### D8：内部模块按职责与状态所有权划分（2026-10-01）

`publish` 拥有发布请求、进度、回执与窄端口，`error` 拥有可进入 IPC 的安全错误上下文。
`CloudFileAccess` 保留组合适配器职责；其启动恢复仅注入账本查询函数，不直接依赖 SQL。
厂商 HTTP、目录与上传状态的实际所有者分别是 `DriveClient`、`DirectoryTree` 和
`GoogleDrivePublisher`，实现位于 `gdrive/{client,directory,object,upload,checkpoint}`。
续传能力 URL 与目录缓存不出厂商内部模块。阿里云盘未来实现发布端口，拥有自己的 HTTP、
上传会话和检查点模型；不强制复用 Google 的协议形态。

公开错误保留 provider、账户、操作、恢复动作及可否重试；网络/限速重试耗尽后返回
保守的 30 秒退避建议。放弃云接收先清理暂存与检查点，再提交取消终态，清理失败仍可重试。
