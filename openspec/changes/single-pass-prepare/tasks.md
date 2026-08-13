## 1. 基线与准备

- [x] 1.1 调用 `/dev-workflow` 加载知识库（本 change 触及 `crates/transfer`、`crates/net` 边界与三端，`net-kernel.md` / `rust-backend.md` / `web-app-frontend.md` / `theme-and-styling.md` 都相关）
- [x] 1.2 建立测试基线：`cargo test --workspace` 与 `./scripts/check-wasm.sh` 各跑一遍并记录当前结果，用于区分「本次改红的」与「原本就红的」
- [x] 1.3 记录改动前的 `crates/transfer/src/bao.rs` 十条测试清单与各自意图（design.md 已逐条推演过命运，落地时对照）

## 2. 验签内核：chunk group 对齐与有效性判据

- [x] 2.1 `crates/transfer/src/bao.rs:41` 的 `BLOCK_SIZE` 改为 `BlockSize::from_chunk_log(8)`，并把 39-40 行的注释从「CHUNK_SIZE 是其整数倍，fetch_plan 天然对齐」改写为「与 CHUNK_SIZE 相等，每个传输块恰好一个叶子」，同时记下为什么不再需要 iroh 的 16 KiB 同款值
- [x] 2.2 新增 `pub fn outboard_len(size: u64) -> u64`（`BaoTree::new(size, BLOCK_SIZE).outboard_size()` 的薄封装），doc 写明「长度即格式版本号：chunk group 变更后旧 outboard 由长度不符自动判失效，故不需要数据迁移」
- [x] 2.3 `encode_proof` 开头加显式对齐校验：`offset % chunk_group_bytes == 0` 且（`(offset+len) % chunk_group_bytes == 0` 或 `offset+len == file_size`），违反时返回可读的 `AppError::Transfer`。**判据必须允许跨多个整叶子**——`tampered_block_is_rejected` 的块长 312 320 B 就是合法的跨叶子输入
- [x] 2.4 修 `overlong_host_read_is_rejected_not_panic`（`bao.rs:383`）：把数据尺寸从 98 061 改到 `CHUNK_SIZE + 12345`，使树有多个叶子、`ignore_len` 的 mock 能触发超长分支。断言的 `"违反契约"` 错误串**必须保留**
- [x] 2.5 改写 `roundtrip_from_16kib_offset`（`bao.rs:398`）：它守护的「非对齐 offset 也能 roundtrip」在新粒度下是假能力（生产 offset 恒为 `CHUNK_SIZE` 倍数，且接收端 `checkpoint.rs:104` 的 `validate_block_range` 早就拒非对齐）。改成断言「非对齐 offset 被 2.3 的校验明确拒绝」，重命名为反映新意图的名字
- [x] 2.6 补边界单测：`size ∈ {0, 1, CHUNK_SIZE-1, CHUNK_SIZE, CHUNK_SIZE+1}` 下断言流式 root == `blake3::hash(data)`。**0 字节那条不能省**——合并后空文件的 checksum 来源从 `hasher.finalize()` 变成 bao root，而流式路径今天没有 `size == 0` 的测试
- [x] 2.7 加护栏测试：用记录调用序列的 mock reader 断言「offset 严格单调递增、每次 len ≤ chunk_group_bytes、len 之和 == 文件大小」。测试名与 doc 注释写明「这条红了说明 bao-tree 换了读取策略，进度单调性与等长判据同时失效」，体例照 `crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs:624-930`
- [x] 2.8 跑 `cargo test -p swarmdrop-transfer bao`，确认其余八条测试如推演所述全绿

## 3. prepare 单遍化

- [x] 3.1 在 `bao.rs` 新增 `ProgressReader<'a, R>` 装饰器（持 `inner: R` 与 `reporter: &'a mut PrepareReporter`），实现 `AsyncSliceReader`。**不加顺序缓冲层**——chunk group 已等于 `CHUNK_SIZE`
- [x] 3.2 新增 `PrepareReporter`：持 `prepared_id`、`events`、跨文件的 `last_emit` 与 `completed_bytes`、当前文件名与文件计数。节流沿用 200ms。**`emit` 的 `Err` 必须 `let _ =` 吞掉**，绝不能转成 `io::Error` 中断构建
- [x] 3.3 新增 `build_outboard_from_source_with_progress`，保留原 `build_outboard_from_source` 给 resume 回填用（两个入口而非 `Option` 参数——给后者塞必填 reporter 会逼 resume 造假 `prepared_id`，让 UI 收到无主进度事件）
- [x] 3.4 `FileAccessReader::read_at`（`bao.rs:88-104`）的守卫从 `chunk.len() > len` 改为 `chunk.len() != len`，并让 reader 带上 `relative_path` 以保住错误可归因。裸 `!=` 的正确性依据：bao 的 `leaf_byte_ranges3` 把叶子 clamp 到 tree size，`offset + len <= size` 恒成立（由 2.7 的护栏测试兜底）
- [x] 3.5 `crates/transfer/src/flow/prepare.rs`：删除 `blake3::Hasher` 循环（40-85）与 `debug_assert_eq!`（96-100），`checksum` 改为 `root.to_hex().to_string()`，`build_outboard_from_source_with_progress` 成为唯一的读取路径。保留末尾那条不受节流的终局事件
- [x] 3.6 确认 `crates/transfer/Cargo.toml:29` 的 `blake3` 仍需保留（`bao.rs` 的 root 类型、`root_from_checksum`、`inbox.rs:149` 的条目指纹都在用），只是 `prepare.rs` 不再直接构造 `Hasher`
- [x] 3.7 跑 `cargo test -p swarmdrop-transfer`，并跑 `cargo test -p swarmdrop-core --test e2e_transfer` 确认端到端仍绿

## 4. outboard 有效性判据落地

- [x] 4.1 `crates/transfer/src/flow/resume/mod.rs:356-372` 的失效判据从 `outboard.is_empty()` 换成 `outboard.len() as u64 != crate::bao::outboard_len(size)`
- [x] 4.2 判据**只放一处**（`build_sender_actor_for_resume` 的回填循环），`plan.rs` 保持摊平 Option 不判可用性。⚠️ **推翻 design.md Open Question 1 的倾向**：`build_prepared_files_from_db` 只有那一个调用者、且 Model 已带 outboard 列，硬塞给 `load_file_outboard` 端口会**多一次 DB 查询**——那是负优化。该端口零消费者一事转记入第 13 组的独立债
- [x] 4.3 补一条测试：喂一个长度不符的非空 outboard，断言被判失效并重算，而不是被原样喂进 `encode_proof`
- [x] 4.4 确认 ≤`CHUNK_SIZE` 的文件（`outboard_len == 0`）在新判据下**不再**每次 resume 都白重读一遍

## 5. 数据面协议名 bump

- [x] 5.1 `crates/transfer/src/protocol.rs`：`TRANSFER_DATA_PROTOCOL` 改为 `/swarmdrop/transfer-data/4`，doc 补上「v4 = v3 + 256 KiB chunk group」以及「验签树形状变更为什么必须换协议名」的推导
- [x] 5.2 删除 `TRANSFER_DATA_PROTOCOL_V2` 常量及其接收侧注册
- [x] 5.3 删除 `wire::data_plane::open_data_stream` 里「拨 `/3` 被拒后退回 `/2`」的回退分支，以及退回后不发窗口帧的那套分支逻辑
- [x] 5.4 全仓 grep `transfer-data/2` 与 `transfer-data/3`，确认没有遗留引用（含测试与文档）
- [x] 5.5 `cargo check --workspace --all-targets` 确认无残留编译引用

## 6. 桌面事件面：Channel → typed event

- [x] 6.1 `src-tauri/src/events.rs` 新增 `PrepareProgress` 的 Event newtype，体例照现有 21 个（`#[serde(transparent)]` + `tauri_specta::Event` derive）
- [x] 6.2 `src-tauri/src/setup.rs:114-136` 的 `collect_events![]` 加入 `events::PrepareProgress`
- [x] 6.3 `src-tauri/src/host/event_bus.rs`：`CoreEvent::PrepareProgress` 分支改为 typed event 广播，删除 `prepare_channels: DashMap` 字段、`PrepareChannelGuard`、以及 `register/unregister` 两个方法
- [x] 6.4 `src-tauri/src/commands/transfer.rs`：`prepare_send` 删除 `on_progress: Channel<PrepareProgressEvent>` 入参与 guard 注册，返回值不变
- [x] 6.5 确认 `src-tauri/src/mcp/tools.rs:358` 的 MCP 路径**无需改动**即获得进度投递（它自己 mint `prepared_id`，广播天然覆盖），并在该处补一行注释说明
- [x] 6.6 `pnpm tauri dev` 跑一次让 `src/lib/bindings.ts` 重新导出（**不要手改**），确认 `events` 对象里出现 `prepareProgress`、`commands.prepareSend` 签名少了 channel 参数

## 7. 桌面前端

- [x] 7.1 新建 store 落点：按 `preparedId` 索引的 `prepares` 记录 + `activePreparedId`，事件到达时自我认领（`preparedId` 拿不到「提前」——事件先于 `prepareSend` 的返回值）。遵守 zustand 两条规则，`pnpm check:zustand-access` 必须绿
- [x] 7.2 `src/routes/_app/send/index.lazy.tsx` 与 `share-target.lazy.tsx`：删掉局部 `useState<PrepareProgress>` 与 Channel 构造，改读 store；渲染门从 `sending && prepareProgress` 改成 store 的活跃标记
- [x] 7.3 发送流程 `finally` 里清除该 `preparedId` 的条目与活跃标记，使下一次发送的头 200ms 不再显示上一批残留
- [x] 7.4 修「进度条停在 100% 但文案还在说『正在计算校验和』」——`startSend` 与 `loadProjections` 期间底部仍停在 prepare 进度条上（`index.lazy.tsx` 的 `finally` 在两步之后才清）。给这两步一个自己的文案，或提前清 prepare 态
- [x] 7.5 进度条改为**叠在按钮之上**而非替换它们（两个发送页同此形态），准备期间界面不再一个可交互元素都不剩。**真正的「取消准备」留作独立 change**：`prepare` 目前没有 AbortSignal 贯穿，按钮标「取消」却取消不了是撒谎——现在它是 disabled 的，诚实。已记入第 13 组

## 8. 移动端

- [x] 8.1 `mobile/src/core/event-bus.ts` 的 `routeEventToStores` 补 `case MobileCoreEvent_Tags.PrepareProgress`，写法对齐既有的 `updateProgress`
- [x] 8.2 同表把同样漏网的 `PairingCompleted` 一并处理（补 case 或改成带注释的显式空 case），恢复 `default` 分支「真的遇到未知事件」的信号价值——仓里表达「决策不落」的方式是带注释的显式空 case（`TransferAccepted` / `TransferResumed` 即是）
- [x] 8.3 `mobile/src/stores/transfer-store.ts` 加 `preparesByPreparedId` + `activePrepareId`，清理点放 `startSend` 的 `finally`，对齐 `applyProjection` 里删 `progressBySession` 的裁剪体例
- [x] 8.4 `send/select-device.tsx` 与 `send/share-target.tsx` 删掉各自的 `useEffect` + `useState`，改读 store；**统一两页今天不一致的渲染门**（一个是 `prepareProgress ?`，另一个是 `sending && prepareProgress`）
- [x] 8.5 两个进度条组件（`PrepareProgressBar` / `SharePrepareProgress`）合一，文案取其一并同步三份 catalog
- [x] 8.6 `cd mobile && pnpm typecheck`

## 9. Web 端

- [x] 9.1 `docs/app/app/_lib/store.ts`：启用已存在但零读者的 `prepares` 表，删除 `latestPrepareProgress` 旁路，新增 `activePreparedId`（首条事件自我认领）
- [x] 9.2 加 `clearPrepare(preparedId)` action，体例对齐 `removeOffer` / `removeProjection`（含 `if (!(id in s.prepares)) return s` 短路——zustand 里「内容没变」要 `return s` 而不是 `return {}`）
- [x] 9.3 `send-panel.tsx` 渲染门从组件局部的 `sendAction.pending` 改成 store 的 `activePreparedId !== null`，使进度跨路由存活
- [x] 9.4 `forward-to-device-dialog.tsx`（收件箱转发）接上同一份进度呈现——它今天走同一个 `send_files` 却只有一个 loader 转圈
- [x] 9.5 补 store 测试（`prepares` 今天零测试）：终态裁剪、并发两个 `preparedId` 不互相覆盖、`clearPrepare` 的短路
- [x] 9.6 确认**不需要**提 `DB_VERSION`：`FileRowDef.outboard` 是 `#[serde(skip)]`（`crates/web/src/store.rs:891`），outboard 从来没进过 IndexedDB。在 change 记录里写死这条，免得后来者白提一版
- [x] 9.7 `cd docs && pnpm build:wasm && pnpm typecheck && pnpm test`

## 10. 顺带清理

- [x] 10.1 删除 `src-tauri/src/host/file_source/path_ops.rs` 的 `compute_hash` / `compute_hash_sync` / `compute_hash_with_progress` / `compute_hash_sync_with_progress` / `verify_checksum` 与 `file_source.rs` 里对应的两个方法（零外部调用者，是接收侧 `receive-staging-publish` 那次删除留下的残骸）
- [x] 10.2 删除 `src-tauri/Cargo.toml:58` 的 `blake3` 依赖（10.1 之后无消费者）
- [x] 10.3 ~~`crates/core` 的 `blake3` 移入 `[dev-dependencies]`~~ —— **前提不成立，无需改动**：第 54 行本就在 `[dev-dependencies]` 段内（tasks 写这条时看错了段落）
- [x] 10.4 桌面 `scanSources` 阶段补 loading 态——那才是真正的「选完文件之后」，大目录时今天纯粹无响应（`-use-file-selection.ts` 的 `FileSelection` 接口没有 `isScanning`）

## 11. 文档同步

- [x] 11.1 修正五处「outboard 与 checksum 同一遍构建」的断言：`dev-notes/knowledge/iroh-migration.md:228`（并删掉与它自相矛盾的 237-238「留作优化」，改成「已落地」）、`net-kernel.md:1409`、`crates/transfer/src/wire/mod.rs:22`、`crates/entity/src/transfer_file.rs:55`
- [x] 11.2 `dev-notes/knowledge/rust-backend.md:604-609` 讲的「16 KiB leaf 粒度非对齐 offset 一读就炸、≤16KiB 文件恰好读对」在新粒度下每个数字都错，整段重写
- [x] 11.3 `net-kernel.md` 记入两条已知负债：验签粒度 256 KiB 是单向门（将来做 range 请求 / 部分文件预览 / iroh-blobs 互通时是硬下限）；`bao 顺序读` 是实现事实、由 2.7 的护栏测试兜底
- [x] 11.4 `crates/host/src/ports.rs:230`、`mobile-core/src/file_access.rs:139`、`src-tauri/src/host/file_source/path_ops.rs:81-84` 三处「按 16 KiB 非对齐 offset 读」的契约说明改为 `CHUNK_SIZE` 粒度。注意契约本身（精确读任意 offset/length）**不变**，变的只是举例的粒度
- [x] 11.5 `crates/web/README.md`、`openspec/changes/receive-staging-publish/design.md:223`、`dev-notes/prompts/web-transfer-pause-resume-verify.md:33-36` 的相关描述
- [x] 11.6 `docs/content/docs/security.mdx:48`（**用户可见**）：改数字之余，考虑换成更好懂的「验签粒度 == 传输块粒度」表述
- [x] 11.7 `dev-notes/blogs/transfer/01-bao-tree-per-chunk-verify.md` 与 `transfer-architecture/04-bao-tree-verified-streaming.md` 的论证整节反转——**补后续篇而非原地改写**，「为什么当初选 16 KiB、后来为什么改」正是最值得留档的那类推导。原文里已漂移的行号引用（`prepare.rs:87`、`bao.rs:284`、`lib.rs:40`、已不存在的 `m20260718_000001_transfer_file_outboard.rs`）顺手修
- [x] 11.8 更新 CLAUDE.md 中与 bao / prepare 相关的描述（若有）

## 12b. `/simplify` 与 `/code-review` 追加（审查发现，非原提案范围）

- [x] 12b.1 三端 store 从「`preparedId → 快照` 表 + 活跃 id」收成**单个 `activePrepare` 字段**——那张表唯一的读者本就是活跃的那一条，非活跃条目从没被读过却要付无上界增长的代价
- [x] 12b.2 修**四条清理漏路**：桌面/移动的 `preparedId` 只在 `prepareSend` 成功后才赋值，于是 `if (preparedId)` 兜底恒空转、它声称覆盖的失败路径一次没覆盖到；Web 只挂 `onSuccess`；MCP 没有前端调用点。`clearPrepare()` 三端统一无参、无条件放 settle；`useAsyncAction` 加 `onSettled`
- [x] 12b.3 修**活跃位被永久占住**：认领规则加「上一批已跑到 100% 就让位」，覆盖 MCP 这类没有调用点的发起方
- [x] 12b.4 桌面进度条改用共享 `calcPercent`（手算不夹上限，`bytesHashed > totalBytes` 时进度条冲出容器）
- [x] 12b.5 `PrepareReporter` 首帧语义对齐 `ProgressTracker`（`Option<Instant>`，第一条必发）
- [x] 12b.6 收尾事件不再把 `current_file` 置空——那个只存在几毫秒的哨兵，代价是三端各一处 UI 分支加 Web 一条 msgid
- [x] 12b.7 `emit` / `emit_final` 共用字段来源；`encode_proof` 的越界与对齐两条判据分开报错；`WindowPacer` 删掉恒定的 `limit` 字段；`&source_id.0.clone()` 的多余分配；移动端两页各 5 个未用 import
- [x] 12b.8 **`UnsupportedProtocol` 有了真正的分类**（新判别码 `FailureCode::PeerProtocolUnsupported`）。此前它被压成 `AppError::Transfer(String)` 走 `Interrupted` → suspended/recoverable，续传机器拿同一个协议名反复重试——**协商阶段确实响亮地失败了，但那份信息在函数内就被字符串化，没有消费者能据此分支**，用户看到的仍是「传输老是断」。这条直接决定 D4 的论证成不成立
- [x] 12b.9 **`validate_fetch_plan` 补对齐校验**。它查了 file_id、溢出、`end > size`、`length == 0`，唯独没查对齐；chunk group 对齐后那 16 倍冗余归零，于是对端提交一个非对齐 offset 的 `ResumeCommit` 会让本端接受、建 actor、开流、读盘，直到 `encode_proof` 抛错 → abort → Interrupted → 对端再提交一次。**这是本次改动新暴露的洞**
- [x] 12b.10 对齐判据收成一处 `is_chunk_aligned_range`（`lib.rs`），三个消费者共用：接收侧 `validate_block_range`、发送侧 `validate_fetch_plan`、`bao::encode_proof`

## 12c. `/code-review high` 追加

- [x] 12c.1 **摘除旧协议名的措辞不准确**：`protocol.rs` / `runtime.rs` / 博客都写着「本仓当时尚无需要兼容的存量用户」，而 SwarmHive 上 v0.12.1–v0.14.0（含 mobile-v*）都已发布。改写成「有意识地放弃向后兼容，因为用户基数小到不值得为兼容付代价」，并把**反方向补不了**这条真实代价写进去（旧发送端只会拨 `/3` `/2`，被拒后按它自己那一版的逻辑无限重试，那一版已经发出去了）
- [x] 12c.2 **新发送端推 fatal 时不通知对端**：控制面协议没变，对端已在 Offer 那步接受、正等数据面连进来；而接收侧只有**启动时**回收过期会话（`stale-receive-session-expiry`），没有运行期空闲超时——不发这一帧它能挂到下次重启。改为推终态前经控制面发 Cancel；`notify_cancel` 的 `reason` 随之参数化（它会经 `TransferFailed` 直达对端用户，说「用户取消」是撒谎）
- [x] 12c.3 **移动端两页仍是「替换」而非「叠加」**：桌面在 7.5 改成了叠加，8.4 的「统一两页渲染门」却统一到了替换式，还去掉了原来的 `sending &&` 联锁。一个没收干净的 `activePrepare` 会**永久顶掉发送与取消按钮**，只能重启应用。两页改为叠加（`BottomActionBar` 内包一层 `flex-col`，不动共享组件）
- [x] 12c.4 **对齐判据的第四个消费者被漏了**：`ReceiverActor::validate_fetch_plan`（对端 Hello 里的计划）没有对齐校验，于是非对齐计划会被接受、读循环起来、第一个 `BlockData` 撞 `validate_block_range` → Abort → 对端拿同一个 Hello 再连。补上，并把 `is_chunk_aligned_range` 的 doc 从「三个消费者」改成四个（两道协商级 + 两道块级防御）
- [x] 12c.5 **中途失败的批次会挡住后续所有进度**：认领规则只让「已跑到 100%」的让位，而 MCP 发起的准备没有前端调用点、失败时停在半路。四处调用点改为**发送开工先清**
- [x] 12c.6 **迟到的收尾事件会重新占住活跃位**：事件是广播的，投递路径与命令返回值不同、顺序无保证。三端 store 加 `clearedPreparedId` 挡住刚清批次的迟到事件（Web 的测试当场抓到 `beforeEach` 漏重置这个新字段，正说明它有效）
- [x] 12c.7 **短读的错误措辞武断归因宿主**：源文件在 `scan_sources` 与 `prepare` 之间被截短也会触发它，说「违反契约」把排查引向适配层。改成「读取长度与预期不符（源文件可能已变更，或宿主违约）」
- [x] 12c.8 移动端共享进度条的注释还在说「收尾事件把 currentFile 置空」——那正是本 change 删掉的行为；`PrepareProgressRow` 删掉从没被传过、且会**替换**而非合并布局类的 `className` prop
- [ ] 12c.9 ⚠️ **仓库根目录有两个 untracked 截图**（`image.png`、`image copy.png`，约 870 KB，非本 change 产物）。`.gitignore` 未覆盖，提交时一个 `git add -A` 就会把它们永久写进历史。**留给你处置**——我不删你的文件

## 12. 门禁与验收

- [x] 12.1 `cargo fmt --all` + `cargo clippy --workspace` + `cargo test --workspace`
- [x] 12.2 `./scripts/check-wasm.sh` 与 `./scripts/check-wasm.sh --clippy`（**本 change 的硬门**——`crates/transfer` 是 wasm 一等公民，BLOCK_SIZE 与 reader 装饰器都在门内）
- [x] 12.3 `./scripts/test-wasm.sh`
- [x] 12.4 `pnpm check:zustand-access`、`pnpm check:shared-view`、`pnpm test`
- [ ] 12.5 手动验收（桌面）：发一个 >100 MB 的文件，确认进度条平滑走到 100% **没有中途停滞**，且 100% 之后立刻进入传输而不是再等一段
- [ ] 12.6 手动验收（跨端）：确认新旧版本互连时得到的是**协议协商失败**，而不是连上后第一个块验签失败的重试循环
- [ ] 12.7 手动验收（跨页面）：prepare 进行中离开发送页再回来，进度仍在；MCP `send_files` 与 Web 收件箱转发也能看到进度
- [ ] 12.8 走 `/simplify` 与 `/code-review` 两道关

## 13. 本 change 不做、但已确认存在的债（留记录）

- [ ] 13.1 `send.rs:95-100` 在 Offer 发出**之前**就把 outboard 落库，对端拒绝也白写
- [ ] 13.2 `storage-sql/ops.rs:189-202` 的 `update_file` 是整行 load→save，outboard 落库后每次发送侧进度落盘都要把 BLOB 读一遍
- [ ] 13.3 移动端 prepare 期间没有任何跨页面表面——前台服务通知只在 `TransferProgress` 上更新（`event-bus.ts:121`），大目录 hash 时切后台仍可能被系统回收
- [ ] 13.4 `openspec/specs/transfer-protocol/spec.md` 已漂移（仍写 `checksum` 是 SHA256 hex、`OfferResult` 携带 256-bit 对称密钥，两者 wire v2 都已不成立）
- [ ] 13.5 `packages/file-browser` 侧的既有断言错误：`mobile/src/components/file-browser/adapters.ts:101` 写「发送来源在所有选择路径下都是可渲染的 `file://`」，而 Android 目录路径恒为 `content://`，缩略图在那条路上静默 fallback 到图标
- [ ] 13.6 **`prepare` 没有取消通道**。三端点了发送之后取消按钮只能 disabled——`TransferManager::prepare` 不收 `AbortSignal`/`CancellationToken`，在途的准备无法中止，用户导航走它也照跑不误（桌面还会把整份 `PreparedTransfer` 连同 outboard 在 `prepared` DashMap 里驻留最多 5-6 分钟）。这是本 change 里唯一「看得见但没修」的 UX 缺口
- [ ] 13.7 `crates/host/src/device_config_file.rs` 的两条测试用**固定路径**的临时目录（`swarmdrop_test_device_config_*`）。注释说「每个用例一个独立子目录，同名会互相踩」——它防了用例之间，没防**进程之间**：并发跑两轮 `cargo test --workspace` 时 `save_then_load_roundtrips_device_name` 会失败。同形态的还有 `src-tauri/src/host/file_source/path_ops.rs` 的几条
- [ ] 13.8 仓库 `target/` 目录在本次实现期间达到 **209 GB**（`debug/deps` 148 GB、`debug/incremental` 17 GB、四个移动端交叉编译 target 共 ~30 GB），把磁盘占到只剩 1.6 GB 并直接编译失败（`No space left on device`）。本次只清了完全可弃的 `incremental`。值得加一条定期 `cargo sweep` 或 CI 之外的本地清理约定
