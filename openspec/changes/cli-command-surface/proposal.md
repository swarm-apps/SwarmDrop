## Why

CLI 的命令面在五条命令里长出了**四种命名风格**（`start`/`stop`/`status` 平动词、
`pair [INVITE]` 一个命令干两件相反的事、`devices` 复数名词、`inbox list|get|export`
noun-verb），而 `inbox` 那条已经是对的形态，只是没人跟。

更深的一处是**数据获取策略有两套、各自手写**：`devices` 走 `Session`（无常驻节点时起一个
完整 P2P 节点，连引导节点 + NAT 探测，几秒），`inbox` 走手写的 `is_alive` + 直连 SQLite
（秒回）。后者是对的——`cmd/inbox.rs` 的模块注释把理由写得很清楚（不该为看一眼收件箱起
P2P 节点，且 migration 连接不设 `journal_mode`，常驻节点在写时直连会撞 `database is
locked`）——但它没有被抽象出来，于是**每加一条命令都要重做一次「我该不该起节点」的判断，
而判断错了不报错，只是白等几秒**。

同时 CLI 缺三项三端都有、core 也都已备好 API 的能力：邀请清单与撤销（`list_invites` /
`revoke_invite_by_hash` 零调用）、解除配对（`paired_devices::unpair` 零调用）、传输历史
（`list_transfer_projections` 零调用）。其中邀请清单是**唯一的止损手段**：邀请 TTL 24 小时、
跨重启存活，泄露后眼下只能 `swarmdrop stop`，而那**根本不管用**——邀请已经落盘，重启回来
它还在。

**现在是唯一的零成本窗口**：`cli/v0.1.0` 从未发布（tag `cli/v*` 一次都没打过），没有任何
存量脚本依赖现有命令名。发布之后再改就是 Docker 那种双轨债——`docker ps` 与
`docker container ls` 至今并存，短的那个赢了，新的那套没人用。

## What Changes

- **BREAKING：命令树全面 noun-verb 化。** 分界线写进规格，不靠约定俗成：**程序自身的操作用
  平动词**（节点是单例，没有集合——tailscale 就是这个形态）、**对象集合用 noun-verb**
  （gh / docker / aws / gcloud 的形态，clig.dev 亦称 `noun verb` 更常见）、**`send` 例外地
  平放**，因为它的对象是文件而文件不归本程序管理，没有对应的名词空间。

  ```
  swarmdrop start / stop / status          节点生命周期（不变）
  swarmdrop send <FILES...> --to <DEVICE>  核心动作（不变）
  swarmdrop invite  create | use | list | revoke
  swarmdrop device  list | forget
  swarmdrop inbox   list | show | export
  swarmdrop transfer list | show
  ```

- **BREAKING：`pair` 整体消失，拆进 `invite` 名词空间。** 现在的 `pair [INVITE]` 靠位置参数
  在「生成邀请并守着」与「用别人的邀请去配对」之间分叉，是两件方向相反的事。拆成
  `invite create` 与 `invite use`。**不留 `pair` 别名**——留了它就赢，`invite create` 会变成
  没人用的正式写法。
  用 `use` 而非 `accept`：`accept` 在本仓已被 `--auto-accept`（自动放行**来敲门的设备**）占用，
  方向相反，同树共存必然误读。

- **BREAKING：三处细节对齐。** `devices` → `device list`（单数 + 显式动词）；
  `inbox get` → `inbox show`（`get` 在 kubectl 里是「列出多个」的意思，误导）；
  `inbox export <ID> --to <DIR>` → `inbox export <ID> <DIR>`（解掉 `--to` 双关——
  `send --to` 是设备，这里是目录，两者类型不同却同名）。

- **新增：两个取数入口取代「`Session` + 手写 `is_alive`」两套。** `RecordAccess`
  （只碰本机记录，**永不起节点**）与 `NodeAccess`（要发包）。分界线一句话说得清：**只有真的要往网络上
  发包的命令需要节点**（`send` / `invite create` / `invite use`），所有「看」与「改本地记录」
  的都不需要。副产品是 `cmd/devices.rs` 那段易错的 `Some(Response::Ok) | None` 兜底样板消失。

- **新增三条能力，core API 全部现成**：`invite list` / `invite revoke`、`device forget`、
  `transfer list` / `transfer show`。
  `invite list` / `revoke` 的无节点路径**零新 API**：建一个孤立的 `InviteRegistry` +
  `load(now)`（正是节点启动时做的事），领域规则（过期过滤、Revoked 过滤、倒序）自动复用，
  不会长出第二份实现。

- **新增：参数不全时的交互补全（clap × dialoguer）。** 三态：给了参数直接执行（人和机器
  同一条路）；没给且能问则交互选择；**没给且不能问一律报用法错误退出，绝不挂着等**——
  dialoguer 在非 TTY 下可能去读一个永不到来的 stdin，在 CI 或管道里就是永久挂死且日志无异常。
  同样不猜默认值：撤销不可逆。

- **新增全局 `--no-input`**（clig.dev 建议）：TTY 检测有测不准的场景（部分 CI 分配伪终端），
  显式开关是脚本与 agent 的逃生口。它与 `--auto-accept` **方向相反**，规格要写死：
  `--no-input` 遇到入站配对请求**拒绝**（fail-closed），`--auto-accept` **接受**（fail-open）。

**非目标**：不动桌面 / 移动 / Web 的任何行为。CLI 做对之后会出现一处新的三端不一致——
桌面与 Web 的邀请清单和设备列表在节点未运行时不可用（`with_manager!` / `node_not_started`，
而库里明明有记录），CLI 将是第一个做对的宿主。**该问题另开 change 处理**，本 change 只在
规格里如实记录这处差异，不去碰已发布的桌面端。

## Capabilities

### New Capabilities

- `cli-command-surface`: CLI 命令面的组织契约——noun-verb 分界线与它的三条判据、命令的资源
  两个取数入口及其可判定的归属规则（**这条命令会不会导致一个数据包离开本机？**）、
  参数不全时的交互三态、
  `--no-input` 与 `--auto-accept` 的方向对立、以及「无节点可读」这条与其余宿主暂时分叉的
  行为承诺。

### Modified Capabilities

无。

**命令改名不触及 `cli-host`**：那份规格通篇不写具体命令名，写的是「设备列表命令」「发送命令」
「配对生成」这类角色描述（唯一的字面量是 `start`，而它不改名）。这正是规格该有的样子——
命令名属于实现，行为契约不该随它变动。

唯一受影响的是一处**行为**而非命名：`### Requirement: 设备列表列的是已配对设备` 的
`#### Scenario: 配对后立即查询` 带着括号说明「此时节点是新起的临时节点」，而设备列表归入
`Persisted` 之后不再起节点。断言本身（刚配对的设备出现在列表中）不但仍成立，还更强了
——它读的是持久化的配对表，不再依赖节点刚起来时的内存态。

这一处**就地更新**，不走 delta：`cli-host` 尚未归档到 `openspec/specs/`，从 OpenSpec 的
视角看那个 capability 还不存在，对它声明 MODIFIED 会得到一个永远合并不进去的 delta
（实测 `openspec validate` 放行，卡在 `archive`），而 `standalone-cli-host` 剩余 5 条任务
全是明确推迟或移交、短期不会归档。CLI 从未发布，那份规格仍是草稿，最终态就该写成最终态。

## Impact

**新增**
- `crates/cli/src/cmd/invite.rs`、`device.rs`、`transfer.rs`（替换 `pair.rs` / `devices.rs`）
- `crates/cli/src/runtime/` 下的资源需求解析入口（取代 `session.rs` 的单一形态 +
  `cmd/inbox.rs` 里手写的那份）
- `crates/cli/src/render/` 对应的三套渲染

**修改**
- `crates/cli/src/cmd/mod.rs`：命令枚举整体重写；`is_interactive()` 要下沉到子命令层
  （`invite create` 与无 ID 的 `invite revoke` 是交互的、`invite list` 不是），
  它有一条断言测试看守（`pairing_is_quiet_by_default`），测试名与参数同改
- `crates/cli/src/runtime/ipc.rs`：动词集随命令面重整（新增邀请清单/撤销、解除配对、传输历史）
- `crates/cli/src/runtime/session.rs`：并入资源需求解析
- `crates/cli/src/cmd/inbox.rs`：手写的双路径取回改为走统一入口
- `crates/cli/src/prompt.rs`：`--no-input` 纳入 `can_ask()` 判据
- `dev-notes/knowledge/cli-host.md`：命令面小节重写
- `openspec/changes/standalone-cli-host/`：10.11 移交本 change；其 `cli-host` spec 中
  「配对后立即查询」那条 Scenario 的括号说明**就地更新**（仅此一处，理由见 Capabilities）

**不修改**
- `crates/core` / `crates/invite` / `crates/transfer` / `crates/host-fs`：本次所需 API
  全部现成，一行不改
- 桌面 / 移动 / Web 三端

**无归档顺序依赖**：本 change 不产出 `cli-host` 的 delta（理由见 Capabilities），
因此它可以先于 `standalone-cli-host` 归档。代价是那一处 Scenario 说明要**与实现同 PR
就地更新**，没有 `openspec validate` 兜底，须落成显式任务。
