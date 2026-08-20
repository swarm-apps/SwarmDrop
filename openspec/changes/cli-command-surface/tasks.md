## 1. 资源需求三档（基础抽象，先行；本组不改任何命令名）

- [x] 1.1 定义资源需求的三档与统一解析入口，语义按 design D4：`Persisted` 常驻走通道、
      否则直连数据库且**永不起节点**；`Live` 常驻走通道、否则起临时节点；`Local` 什么都不要
- [x] 1.2 解析入口同时接收「通道请求」与「无常驻时的本地取数」，由它决定走哪条——
      消除 `cmd/devices.rs` 那个 `Some(Response::Ok) | None` 兜底样板（它处理的是「通道刚才
      还在、这一瞬没了」的竞态，每条命令各写一遍时漏掉的那条会在节点关停瞬间报错错原因）
- [x] 1.3 把 `runtime/session.rs` 的 `Acquisition` 二态并入解析入口，`Session` 不再是命令层
      直接面对的抽象
- [x] 1.4 把 `cmd/inbox.rs` 里手写的 `is_alive` + 直连数据库迁到统一入口，**保留它原有的
      正确性依据**（`migration` 连接不设 `journal_mode`，常驻节点写入时直连会撞
      `database is locked`）并把该依据写进入口的模块文档
- [x] 1.5 设备列表改判为 `Persisted`：读本机已配对设备记录，不再起临时节点
- [x] 1.6 无节点时设备的在线状态呈现为「未知」，不得呈现为「离线」——临时节点刚起来时
      presence 探测尚未完成，那个值本就不可信（design D12）
- [x] 1.7 为每条命令的档位归属写断言测试：档位是命令定义的一部分，改动它必须撞到测试
      （归属错误的表现是「命令跑完了但包没发出去」，不报错）
- [x] 1.8 `cargo test --workspace` 全绿，确认本组重构未改变任何既有命令的可观察行为

## 2. 命令树重整（BREAKING；CLI 从未发布，无迁移）

- [x] 2.1 重写命令枚举，落地 design D1 的三条规则：`start`/`stop`/`status` 与 `send` 平铺；
      `invite` / `device` / `inbox` / `transfer` 为两级名词
- [x] 2.2 `pair [INVITE]` 拆为 `invite create` 与 `invite use`——现有形态靠位置参数在「生成并
      守着」与「用别人的邀请去配对」之间分叉，是两件方向相反的事
- [x] 2.3 **不保留 `pair` 别名**（不用 clap 的 `visible_alias`）：留了它就赢，`invite create`
      会变成没人用的正式写法（design D2 的 Docker 双轨论证）
- [x] 2.4 `devices` → `device list`；`inbox get` → `inbox show`（`get` 在 kubectl 里是「列出
      多个」的意思）；`inbox export <ID> --to <DIR>` → `inbox export <ID> <DIR>`（解掉 `--to`
      双关：`send --to` 是设备，这里是目录）
- [x] 2.5 `is_interactive()` 下沉到子命令层——`invite create` 与无 ID 的 `invite revoke` 是
      交互的、`invite list` 不是。保持穷尽 match（新增交互命令时编译失败，而不是带着一屏
      滚动日志上线）
- [x] 2.6 更新 `cmd/mod.rs` 的 4 条 clap 测试，其中 `pairing_is_quiet_by_default` 连测试名
      一起改；`default_filter_covers_this_crate` 断言的 bin 名不变，确认它仍绿
- [x] 2.7 顶层 `long_about` 给一句上手路径，`invite` 的 `about` 写成「配对邀请」而非「邀请」
      ——删掉 `pair` 之后「配对」这个心智在顶层帮助里没有直接对应，这是唯一的缓解手段
- [x] 2.8 确认命令层级不超过两级，且没有任何集合动作以 `--list` / `--revoke` 这类开关表达

## 3. 邀请清单与撤销（移交自 standalone-cli-host 的 10.11）

- [x] 3.1 无节点路径：建一个孤立的 `InviteRegistry` + `load(now)` 再操作，**不直接查
      `invite` 表**——领域规则（未过期、非已撤销、按创建时刻倒序）住在 `list_active` 里，
      直接查库就要在 CLI 里再写一份（design D5）
- [x] 3.2 确认 `revoke_by_hash` 必须在 `load` 之后调用：它查不到内存记录会直接 no-op 并
      **报告成功**，不 `load` 就撤销等于「报告成功但什么都没发生」
- [x] 3.3 新增邀请清单与撤销两个通道动词，`NodeHandler` 侧只做「调能力函数 + 序列化」
- [x] 3.4 `invite list` 渲染：标识、创建时刻、过期时刻、是否已被使用；**输出不含邀请串本身**
      （凭证明文不落盘，重启后无法重建）
- [x] 3.5 `invite revoke <ID>` 接受标识的唯一前缀，下限 4 位；前缀撞车时列出候选并以用法
      错误退出，**不在候选中任选其一**（撤销不可逆，歧义不得由系统代为消解）
- [x] 3.6 撤销如实报告持久化结果：未落盘时仍以成功退出（动作确实完成了），但在诊断输出中
      明确告知「重启后该邀请会重新可用」
- [x] 3.7 `invite revoke --all`：可交互时确认一次并展示将撤销的数量，`--yes` 跳过；不可交互
      且无 `--yes` 时以用法错误退出（design D11 的风险分级）
- [x] 3.8 `invite create` 输出中给出该邀请的标识——列表不含邀请串，一分钟内发两张时仅凭
      时刻无法分辨，这是「刚发错人、立刻撤回」可用的前提（design D10）
- [x] 3.9 测试：无节点时列出与撤销、撤销后该邀请不再可配对、撤销跨重启生效、前缀唯一与
      撞车两条路径、未落盘时的告知

## 4. 解除配对

- [x] 4.1 `device forget <DEVICE>`，走 `Persisted` 档
- [x] 4.2 无节点分支需为 `unpair` 的泛型显式标注（签名是 `net: Option<&NetManager<T>>`，
      传 `None` 时 `T` 推不出来）——用一个类型别名收口，不在每个调用点重复那串类型
- [x] 4.3 文案呈现为**单方面**操作：移除的是本机对该设备的记录，对端是否仍记着不在本命令
      控制范围内。用词选 `forget` 而非 `unpair`，与该语义一致
- [x] 4.4 测试：无节点时解除成功且不起节点；有节点时额外停止对该设备的在线状态探测

## 5. 传输记录

- [x] 5.1 `transfer list`，走 `Persisted` 档；输出按开始时刻倒序（端口契约已保证该序，
      不在 CLI 侧重排）
- [x] 5.2 `transfer show <ID>` 详情
- [x] 5.3 渲染两套输出：人类可读与结构化，进度与诊断走标准错误
- [x] 5.4 测试：无节点时可列出、倒序、结构化输出可解析

## 6. 交互层（clap × dialoguer）

- [x] 6.1 新增全局 `--no-input`，纳入 `prompt::can_ask()` 的判据（现有两条是
      stdin 与 stderr 均为 TTY）
- [x] 6.2 落地三态：给了参数直接执行 / 没给且能问则交互 / 没给且不能问报用法错误退出。
      **人与程序走同一条命令**，不为程序化调用另开一套
- [x] 6.3 不可交互时**立即退出，绝不读标准输入**——dialoguer 在非 TTY 下可能去读一个永不
      到来的 stdin，在管道或 CI 中表现为永久挂起且日志无异常
- [x] 6.4 同样不猜默认值（如「撤销最新那张」）：撤销不可逆，猜错没有补救
- [x] 6.5 结构化输出模式一票否决交互，即使 stdin 是 TTY
- [x] 6.6 确认 `dialoguer` 的选择组件在当前 features 下可用；交互菜单绘制在标准错误
- [x] 6.7 落地 `--no-input` 与 `--auto-accept` 的方向对立：前者遇入站配对请求**拒绝**、
      后者**接受**；两者同时给出时后者生效
- [x] 6.8 测试：非 TTY 缺参数不挂起且退出码为用法错误、`--no-input` 在 TTY 下同样拒绝交互、
      结构化模式下不出现菜单

## 7. 规格与文档同步

- [x] 7.1 就地更新 `openspec/changes/standalone-cli-host/specs/cli-host/spec.md` 中
      `### Requirement: 设备列表列的是已配对设备` 下 `#### Scenario: 配对后立即查询` 的
      括号说明——「此时节点是新起的临时节点」在设备列表归入 `Persisted` 后失效。
      **该文件其余条目不含命令名，无需改动**
- [x] 7.2 `openspec/changes/standalone-cli-host/tasks.md` 的 10.11 标注为移交本 change
- [x] 7.3 重写 `dev-notes/knowledge/cli-host.md` 的命令面小节：三条分界规则、三档资源需求
      及其可判定问句、交互三态、`--no-input` 与 `--auto-accept` 的方向对立
- [x] 7.4 全仓搜索并更新 `swarmdrop pair` / `swarmdrop devices` / `inbox get` 的出现位置
      （README、文档站、注释、`dist` 相关说明）
- [x] 7.5 在知识库记下本次的两条非显见结论：`cli-host` 规格不写命令名所以命令重整不触及它；
      未归档 capability 的 MODIFIED delta 能过 `validate` 但会卡在 `archive`

## 8. 门禁与真机验证

- [x] 8.1 `cargo fmt --all` / `cargo check --workspace --all-targets` / `cargo clippy -p
      swarmdrop-cli` / `cargo test --workspace` 全绿
- [x] 8.2 真 TTY 下用 vhs 验证四条交互路径：无参数撤销的选择菜单、前缀撞车的候选列表、
      `--all` 的确认、Ctrl-C 中止的退出码
- [x] 8.3 非 TTY 验证：管道中执行缺参数命令**不挂起**且退出码为用法错误（这条只有在真实
      管道里才显形，单测覆盖不到挂起）
- [x] 8.4 无节点验证：停掉节点后 `invite list` / `device list` / `transfer list` /
      `device forget` 均可用且不启动节点（用是否出现监听地址日志佐证）
- [x] 8.5 `openspec validate cli-command-surface --strict` 通过

## 9. 文本投递（2026-08-20 追加）

- [x] 9.1 `send --text [<TEXT>]`：三态 `Option<Option<String>>`（给内容 / 只给开关 /
      不给），与位置参数 `conflicts_with`。**不新建 `text` 名词**——收到的文本进的是
      收件箱，只有发件账本在别处，做出来会是一个只有一半的集合
- [x] 9.2 正文三条来源：命令行 / 标准输入（非终端时读到 EOF，`take(MAX+1)` 后转 UTF-8，
      两种失败不许混成一种）/ `$EDITOR`（dialoguer `editor` feature，为此打开）
- [x] 9.3 空与超限在**起节点之前**判定，退出码 2；措辞两处共用一份（`too_long`）
- [x] 9.4 只削结尾换行，缩进与内部空行原样保留；与 `$EDITOR` 那条路径的结果一致
- [x] 9.5 通道动词 `SendText`，与 `Send` 分开（服务端骨架没有一步重合）
- [x] 9.6 终态分类 → 退出码：`delivered` 成功；`peer_unavailable` / `timed_out` →
      `PeerUnreachable`；其余 → `TransferFailed`。状态名抄自 entity 的 serde 形态，
      由 `text_status_names_match_the_wire` 看守
- [x] 9.7 回执说「已送达」不说「已发送」（spec: text-send-experience）
- [x] 9.8 等待转轮画在**命令层**（两条取数路径都经过），且作用域必须在打印结果之前结束
- [x] 9.9 **常驻节点自动确认入站文本**：`TextDeliveryAttention{ConfirmationRequired}`
      → `service.accept`。判据同文件（已配对）。少了它，默认信任档位
      （`Collaborator` 带 `require_confirmation`）下**一条都收不到**，且失败形态是
      发送端阻塞 5 分钟后报「已过期」
- [x] 9.10 收件箱标题压平成一行（`title_line`）：文本条目的标题带正文里的换行，
      而列表、菜单、详情三处都假定它占一行
- [x] 9.11 测试：三态解析、互斥、正文边界、只削结尾换行、状态名对齐、退出码分类；
      管道中 `send --text` 不挂起（`tests/without_a_node.rs`）
- [x] 9.12 真机验证：两个数据目录起两个节点、配对、行内 / 管道 / `--json` / pty 里的
      `$EDITOR` 四条路径均送达，对端离线时退 4
- [x] 9.13 文档：`docs/content/docs/cli.mdx` 新增「发一段文本」，
      `dev-notes/knowledge/cli-host.md` 记下九条非显见结论
- [x] 9.14 **顺带修掉 `send` 的一个既有缺陷**：`resolve_target` 用的是
      `DeviceFilter::All`（本次运行**发现**的对端），无常驻节点时那张表是空的——
      于是 `send … --to <设备>` 必报「找不到已配对设备」，而 `device list` 明明列着它。
      改成 `Paired`。文件与文本共用这个函数，所以两支一起修好

## 10. 收到的东西在哪儿（2026-08-20 追加）

- [x] 10.1 `inbox show` 增加**位置**一行：单文件取 `localPath`、多文件取 `rootPath`，
      优先级与桌面端 `item_target_path` 一致
- [x] 10.2 `transfer show` 对接收方向增加同一行；发送方向**不呈现**（没有落点，
      印占位符会让人以为记录坏了）
- [x] 10.3 文本条目不呈现位置——它没有本地文件
- [x] 10.4 **不拼接**「根目录 + 相对路径」：各文件 `local_dir` 未必相同，
      core 的 `content_root_of` 在不一致时回退存储根
- [x] 10.5 测试：优先级、缺位置时的占位符、标题压平与截断
- [x] 10.6 真机验证：单文件条目 / 目录条目 / 文本条目 / 发送记录四种形态
