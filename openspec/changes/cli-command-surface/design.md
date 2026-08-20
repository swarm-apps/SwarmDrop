## Context

见 `proposal.md` 的 Why。这里只补三条塑造方案的现状约束：

1. **`Session` 与 `cmd/inbox.rs` 是两套并行的取数逻辑。** 前者是 `Acquisition::{Existing,
   Owner}` 二态（复用常驻 / 起临时节点），后者是手写的 `is_alive()` + 直连 SQLite。
   两者都对，但覆盖的是不同档位的需求，而没有任何东西表达这件事。
2. **`crates/cli` 之外一行不用改。** 三条新能力的 core API 全部现成：
   `InviteRegistry`（已从 `swarmdrop_invite` 导出）、`paired_devices::unpair`
   （签名就收 `net: Option<&NetManager<T>>`）、`TransferStore::list_transfer_projections`
   （契约保证按 `started_at` 倒序）。
3. **`cli-host` 规格通篇不写命令名**（写的是「设备列表命令」「配对生成」这类角色描述），
   所以命令面重整几乎不触及它——唯一受影响的是「配对后立即查询」那条 Scenario 里
   「此时节点是新起的临时节点」的括号说明。该规格尚未归档，那一处就地更新而非走 delta，
   理由见 `proposal.md` 的 Capabilities。

## Goals / Non-Goals

**Goals:**
- 命令面的形状由**可判定的规则**决定，而不是逐条命令的品味。新增命令时不需要重新辩论。
- 「这条命令要不要起节点」从每条命令的私有判断，变成一个声明 + 一处解析。
- 一条命令的本地路径与 IPC 路径不再各写一遍同样的编排。

**Non-Goals:**
- 不设计 `send --detach` 与传输控制（暂停 / 取消 / 续传）。`transfer list` 落地后它才有意义，
  但它需要一套新的会话控制动词，属于另一个 change。
- 不改 IPC 的传输与编码（本地套接字 + 行分隔 JSON 不变），只改动词集。
- 不为 `invite list` 设计跨设备视图。邀请注册表是本机的，别的设备发出的邀请与本机无关。

## Decisions

### D1：命令树的分界线用三条可判定的规则，而不是逐条拍板

```
规则 1  操作对象是「这个程序自身」且是单例  → 平动词
        start / stop / status（节点没有集合，无从 list）

规则 2  操作对象是本程序管理的一个集合      → noun verb
        invite / device / inbox / transfer

规则 3  操作对象不归本程序管理              → 平动词
        send（对象是文件系统里的文件）
```

规则 3 是 `send` 唯一的豁免依据，写下来是为了防止它被读成「高频所以平放」——那条理由不可
判定，下一个人会用它把 `device list` 也拉平。`git push` / `docker run` 同样落在规则 3。

**否决 flag 化**（`invite --list` / `--revoke`）：flag 的语义是修饰而非动作；两个动作 flag
的互斥关系在 `--help` 里看不出来，只能运行时报错；且它偏离了实测的五个主流 CLI 中的四个
（gh / docker / aws / gcloud 皆 noun-verb，仅 kubectl 是 verb-noun）。

**否决三层**（`invite key list` 之类）。gh 的三层只出现在「资源的子资源」上
（`gh repo deploy-key list`），本仓没有那种嵌套。

### D2：`pair` 整体删除，不保留别名

保留别名会得到 Docker 的结局：`docker ps` 与 `docker container ls` 至今并存，短的赢了。
本仓的窗口是**一次性**的——`cli/v0.1.0` 从未发布，tag `cli/v*` 一次都没打过，没有存量脚本。

代价如实记：`swarmdrop pair` 是新用户最可能猜到的第一条命令，删掉它意味着「配对」这个心智
在顶层 `--help` 里没有直接对应。缓解手段是 `invite` 的 `about` 文本写成「配对邀请」而非
「邀请」，以及顶层 `long_about` 给一句上手路径。**不用 clap 的 `visible_alias`** ——
那等于留了别名。

### D3：用 `use` 而非 `accept`

`accept` 在本仓已被占用且方向相反：`--auto-accept` 是「自动放行**来敲门的设备**」，
而 `invite accept <链接>` 会是「拿着码去敲**别人的**门」。同一棵树里两个 accept 指向相反的
角色，必然误读。

`use` 胜过 `redeem`：后者语义更精确（一次性凭证），但英文味重，而 `--help` 的读者要在中文
语境下扫一眼就懂。`invite create` / `invite use` 是优惠券的心智，自洽。

### D4：命令声明资源需求，三档，一处解析

```
Persisted   只读/只改持久化记录  常驻 → IPC；否则 → 直连 SQLite。永不起节点。
Live        要往网络上发包        常驻 → IPC；否则 → 临时节点。
Local       只碰本地文件系统      什么都不要。
```

归属规则可判定：**这条命令会不会导致一个数据包离开本机？** 会则 `Live`，不会则看它读不读库。

| 档 | 命令 |
|---|---|
| `Persisted` | `device list` · `device forget` · `invite list` · `invite revoke` · `inbox list` · `inbox show` · `transfer list` · `transfer show` |
| `Live` | `send` · `invite create` · `invite use` |
| `Local` | `inbox export` 的文件复制部分（取详情那步是 `Persisted`） |
| 自成一路 | `start` / `stop` / `status`——它们操作的是节点本身，不是经由节点取数 |

`Persisted` 直连 SQLite 的**正确性**依据不变（`cmd/inbox.rs` 已有的那条）：`migration` 的连接
不设 `journal_mode`，走 SQLite 的 `delete` 模式，写事务阻塞所有读；常驻节点接收文件时一直在写，
所以有常驻时必须走 IPC。没有常驻时也就没有并发写者，直连是安全的。

**考虑过并否决的替代**：让所有命令统一走 `Live`（形态最简单）。否决理由是它把
`swarmdrop invite list` 变成一条要连引导节点、做 NAT 探测的命令——为看一眼本地记录付几秒，
而这恰恰是「邀请泄露了赶紧撤」那个场景最不能等的地方。

### D5：无节点时的 `invite list` / `revoke` 走一个孤立的 `InviteRegistry`

不直接读写 `invite` 表，而是 `InviteRegistry::new(SqlInviteStore)` → `load(now)` →
`list_active(now)` / `revoke_by_hash(hash)`。这正是节点启动时做的事（`runtime.rs` 的
`load_invites()`）。

两条理由：

1. **领域规则零重复。** 「未过期且非 Revoked、按 `created_at` 倒序」住在 `list_active` 里；
   直接查库就要在 CLI 里再写一份，那是本仓明确反对的形态（收件箱领域规则住
   `crates/transfer/src/inbox.rs` 由各存储实现调用，是同一条约定）。
2. **`revoke_by_hash` 本来就依赖内存表。** 它 `invites.get_mut(&hash)`，查不到直接
   `return true`（no-op）。不 `load` 就撤销，结果是「报告成功但什么都没发生」——最坏的一种
   失败形态。`load` 之后它才有东西可改。

**`crates/invite` 一行不改。**

### D6：本地路径与 IPC 路径合并成一次调用

现状是每条命令写三份：`cmd/` 里的本地分支、`ipc::Request` 的动词、`NodeHandler` 的分支，
而前两者常常调同一个函数（`devices` 的两侧都是 `paired_devices(node)`）。

形态：解析入口同时收「IPC 请求」与「无常驻时的本地取数」，由它决定走哪条：

```rust
// 形态示意，最终签名在实现时定
let payload = resolve(Need::Persisted, Request::InviteList, || load_invites_from_db(dir)).await?;
```

收益不只是行数：`cmd/devices.rs` 现在那个 `Some(Response::Ok) | None =>` 的兜底分支是纯样板，
**写错了编得过**——它要处理「通道刚才还在、这一瞬没了」的竞态，而每条命令各写一遍时，
漏掉的那条会在节点关停的瞬间报一个与真实原因无关的错。合并后这段只存在一处。

`NodeHandler` 那侧仍需为每个动词写分支——那是跨进程的另一端，无法合并，但它只做
「调能力函数 + 序列化」，不含编排。

### D7：参数不全时的交互补全，三态

```
给了参数            → 直接执行（人与机器同一条路，不为 agent 单开命令）
没给 + 能问         → dialoguer 交互选择
没给 + 不能问       → CliError::Usage（退出码 2），提示去看对应的 list
```

`能问` = `stdin` 与 `stderr` 都是 TTY，且 `--json` 未开，且 `--no-input` 未给。前两条是
`prompt::can_ask()` 已有的判据，本次只加第三条。

**「不能问」必须立刻退出，绝不挂着等。** dialoguer 在非 TTY 下可能去读一个永不到来的 stdin，
在 CI 或管道里就是永久挂死，而日志上看不出任何异常——这是本仓在 `pair` 上已经踩过的形态
（`cmd/pair.rs` 的 `if !auto_accept && !can_ask()` 就是那次的产物）。

**同样不猜默认值**（比如「撤销最新那张」）。撤销不可逆，猜错没有补救。

`--json` 一票否决交互，即使 stdin 是 TTY：`--json` 声明的是「我是程序」，而程序不会去读
dialoguer 画在 stderr 上的菜单。

### D8：`--no-input` 与 `--auto-accept` 方向相反，规格写死

| flag | 语义 | 遇到入站配对请求 |
|---|---|---|
| `--no-input` | 不要问我 | **拒绝**（fail-closed） |
| `--auto-accept` | 不用问，一律放行 | **接受**（fail-open） |

这条必须显式，否则一定有人以为 `--no-input` 会让配对「自动通过」。两者同时给出时
`--auto-accept` 生效——它是更具体的指令，而 `--no-input` 只是说「别弹交互」。

### D9：邀请标识接受唯一前缀，撞车一律拒绝

完整 ID 是 `sha256(capability)` 的 64 字符 hex。脚本粘贴无所谓，人手敲不现实。

按 git 的形态接受前缀，但**撞车时不猜**：列出候选并报 `Usage`。邀请通常个位数条（TTL 24 小时，
个人用途），撞车概率接近零——但撤销没有 undo，不能靠概率。前缀下限取 4 位，低于它直接拒绝，
避免「敲了一个字符就撤掉了什么」。

### D10：生成邀请时当场打印它的标识

不是装饰，是 `invite list` 可用的前提。列表里**没有邀请串本身**（`invite-persistence`
design D4：capability 明文不落盘），能显示的只有标识、创建时间、过期时间、是否已消费。
于是「刚发到微信那条是哪张」只能靠时间猜——一分钟内发两张就分不出。

桌面端靠视觉时序绕过（生成后列表自动刷新，最上面那条就是），CLI 没有那个连续上下文，
只能把标识留在终端 scrollback 里。它同时给将来「`invite create --auto-accept` 收窄到刚生成
的这张」留了口子——现在那个 flag 的范围是「任一有效邀请」，包括昨天发出去的。

### D11：`--all` 按 clig.dev 的风险分级处理，单张撤销不确认

| 操作 | 档位 | 处理 |
|---|---|---|
| `invite revoke <ID>` | mild | 不确认。给了 ID 即明确意图，且撤销常常正是紧急止损，多一跳碍事 |
| `invite revoke`（交互选） | mild | 不额外确认。选项里已显示完整信息，选择本身就是确认 |
| `invite revoke --all` | moderate | 能问时确认一次；`--yes` 跳过；不能问且无 `--yes` 时报 `Usage` |

`--all` 单独做的理由：泄露时的真实需求往往是「不知道哪张漏了，全撤」，而眼下唯一的手段
`swarmdrop stop` **根本不管用**——邀请已落盘，重启回来它还在。

### D12：无节点时 `device list` 的在线状态如实报「未知」

已配对设备表在 `paired-devices.json` 里，节点不跑时它照样存在；需要节点的只有在线状态。

但**临时节点刚起来时的在线状态本来就不可信**——presence 探测还没跑完。花几秒起一个节点去
换一个不准的答案，两头不讨好。所以 `Persisted` 路径读设备表、在线状态标为未知，秒回。

这与桌面端当前行为不同（桌面 `list_devices` 在节点未启动时直接 `node_not_started`，
设备页空白）。判据是 CLI 这条对、桌面那条是缺陷，另开 change 处理；本 change 在规格里
如实记录这处分叉，不去碰已发布的桌面端。

与既有规格不冲突：`cli-host` 的「一次性命令使用临时节点」本就限定在「**需要网络的**一次性
命令（如发送）」，D4 的三档是它的细化而非推翻。

## Risks / Trade-offs

**[三端出现新的行为分叉]** CLI 的 `invite list` / `device list` 无节点可用，桌面与 Web 不行。
→ 规格里显式写明这是**有意的先行**而非特例，并另开 change 追平其余宿主。不写的话，下一个人
会把 CLI 这条当成需要「修正」的偏差。

**[`Need` 归属判断错了是静默的]** 把一条 `Live` 命令标成 `Persisted`，表现不是报错而是
「命令跑完了但包没发出去」。→ 归属规则做成一句可判定的问句（D4），并对每条命令的档位写
断言测试；档位是命令定义的一部分，改动它会撞到测试。

**[前缀匹配引入新的失败模式]** 撞车、太短、大小写。→ 一律拒绝并列出候选，不猜；下限 4 位。
代价是极少数情况下用户要多敲几位，可接受。

**[`unpair` 的泛型在无节点分支上要显式标注]** 签名是 `unpair<T: TransferRuntime>(..,
net: Option<&NetManager<T>>)`，传 `None` 时 `T` 推不出来，须写成 `None::<&NetManager<..>>`。
→ 实现时用一个类型别名收口，避免每个调用点各写一遍那串类型。

**[IPC 动词集从 10 个涨到约 16 个]** `NodeHandler::handle` 会更长。→ 它只做「调能力函数 +
序列化」，没有编排；真正的风险是漏注册，由 `Request` 枚举的穷尽 match 兜住（无 `_` 分支）。

**[`cli-host` 那处 Scenario 说明要与实现同 PR 更新]** 不走 delta 意味着没有
`openspec validate` 兜底，而它是一句括号里的说明、极易漏掉。→ 落成 tasks 里的显式条目，
并写明是哪一条 Scenario 的哪一句。

## Migration Plan

**无用户迁移**：`cli/v0.1.0` 从未发布。破坏性改动的成本仅限于本仓内部的引用。

需要同步的落点（漏了不会报错，只会让文档描述一个不存在的命令面）：

1. `openspec/changes/standalone-cli-host/specs/cli-host/spec.md`——仅
   `### Requirement: 设备列表列的是已配对设备` 下 `#### Scenario: 配对后立即查询` 的
   括号说明（其余条目不含命令名，无需改动）
2. `openspec/changes/standalone-cli-host/tasks.md` 的 10.11 标为移交
3. `dev-notes/knowledge/cli-host.md` 的命令面小节
4. `crates/cli` 内所有带命令名字面量的测试（`cmd/mod.rs` 的 4 条 clap 测试首当其冲，
   其中 `pairing_is_quiet_by_default` 连测试名一起改）
5. README 与文档站里出现 `swarmdrop pair` / `swarmdrop devices` 的位置

**回滚**：单个 PR 内的纯 CLI 改动，`git revert` 即可，无数据格式变更、无持久化 schema 变更。
