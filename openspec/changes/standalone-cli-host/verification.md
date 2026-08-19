# Scenario 走查（任务 9.4 产物）

逐条核对 `specs/` 下的全部 scenario。三种状态：
**✅ 已实测**（本机跑过并观察到结果）· **🔵 已实现**（代码与单测覆盖，但需要第二台设备或
一次真实发布才能观察）· **⏸ 待发布**（依赖尚未执行的对外操作）。

## cli-host

| # | Scenario | 状态 | 依据 |
|---|---|---|---|
| 1 | 启动常驻节点 | ✅ | `swarmdrop start -d` 后 `status` 返回 `running`，20 条监听地址 |
| 2 | 停止节点 | ✅ | `stop` 输出「节点已停止」，退出码 0 |
| 3 | 停止时无节点在运行 | ✅ | 输出「当前没有节点在运行」，**退出码 0**（非错误） |
| 4 | 查询状态 | ✅ | 输出含状态 / 节点标识 / NAT / 监听地址；`--json` 可被 `json.load` 解析 |
| 5 | 前台启动可被信号终止 | 🔵 | `cmd/start.rs` 的 `tokio::select!` 收 `ctrl_c` 后走关停路径 |
| 6 | 后台启动立即返回 | ✅ | `start -d` 秒回并打印「节点已在后台启动」 |
| 7 | 后台节点可被另一终端停止 | ✅ | 另一次调用 `stop` 成功停掉后台节点 |
| 8 | 无常驻节点时发送 | 🔵 | `Session::open` 起临时节点；需对端才能观察传输完成 |
| 9 | 临时节点期间的状态查询 | ✅ | 无常驻节点时 `status` 返回 `running`（临时节点确实在线） |
| 10 | 重复启动被拒绝 | ✅ | 输出「节点已在运行」，**退出码 3**（NodeUnavailable） |
| 11 | 常驻节点存在时执行其他命令 | ✅ | 常驻时 `status` / `devices` 经通道复用，未起第二个节点 |
| 12 | 陈旧的通道残留不阻塞启动 | ✅ | 单测 `stale_socket_does_not_block_startup`（写入假 socket 文件后仍能拿到持有权并清理它） |
| 13 | 与同机桌面端并存 | ✅ | 两个独立数据目录各起一个节点，**节点标识不同**、互不干扰 |
| 14 | 独立配对 | ✅ | 数据目录与身份文件各自独立（`identity.json` 不共享） |
| 15 | 常驻期间自动接收 | 🔵 | `runtime/receive.rs` 订阅 `TransferOfferReceived` 后自动 `accept_and_start_receive`；需对端才能观察 |
| 16 | 交互式终端生成邀请 | ✅ | 输出半块字符二维码 + canonical 邀请链接 |
| 17 | 关闭二维码 | ✅ | `--no-qr` 只输出链接 |
| 18 | 接受邀请 | 🔵 | `pair <invite>` 调 `pair_with_invite`；需对端 |
| 19 | 结构化输出可被解析 | ✅ | `status --json` / `devices --json` / `inbox list --json` 均被 `json.load` 成功解析 |
| 20 | 进度不污染标准输出 | ✅ | 日志与进度经 `tracing`/`eprint!` 走 stderr；`2>/dev/null` 后 stdout 仍是完整 JSON |
| 21 | 成功（退出码 0） | ✅ | 实测 |
| 22 | 对端不可达（独立退出码） | 🔵 | `CliError::PeerUnreachable` → 码 4，与 `TransferFailed`（5）分开；单测 `every_failure_has_a_distinct_code` 保证互异 |

**用法错误退出码 2** 亦已实测（`send` 缺参数、`inbox get` 传非法 id）。

## cli-distribution

| # | Scenario | 状态 | 依据 |
|---|---|---|---|
| 1 | CLI 发布不触发桌面发布 | ✅ | 三条触发模式互不重叠：`v*` / `mobile-v*` / `cli**[0-9]+.[0-9]+.[0-9]+*`；`cli/v0.1.0` 不以 `v` 开头 |
| 2 | 桌面发布不触发 CLI 发布 | ✅ | 同上，`v0.23.0` 不以 `cli` 开头 |
| 3 | 标签形态被验证 | ✅ | `dist plan` 成功解析出 `v0.1.0`；斜杠形式的理由记在配置注释与 `knowledge/cli-host.md` |
| 4 | 再生成不破坏既有流水线 | ✅ | `dist generate` 前后 `release.yml` 的 sha256 **逐字未变** |
| 5 | 未装过的机器一条命令装好 | ⏸ | 需先发布。`dist plan` 已确认产出 shell / powershell installer |
| 6 | npm 包可作为依赖被引入 | ⏸ | 需先发布到 npm 的 `swarmdrop` |
| 7 | 每个受支持平台都有产物 | ✅（计划层面） | `dist plan` 列出全部六个目标的归档与校验和 |
| 8 | 通过安装渠道更新 | ✅ | `install-updater = false`，程序自身不提供更新命令 |

## 复查中发现并修掉的四个缺陷

走查 scenario 时对照 spec 逐条核，查出四处实现与规格不符——**没有一处是编译或单测能发现的**：

### 1. `pair` 生成的邀请当场失效（最严重）

无常驻节点时 `pair` 起临时节点签发邀请，**签完就关节点**。而邀请里带的可拨地址就是那个
临时节点的——命令一退出它就没了，用户拿到的是一张**扫了也拨不通的码**，且没有任何报错。

改为：临时节点签发后**保持在线直到配对完成或用户中断**，并在 stderr 上说明「这张码在本
命令退出后即失效」。邀请的有效期本质上等于签发者的在线时长，这条不该让用户自己去推。

### 2. `pair` 在常驻节点存在时直接报错

违反 spec 的「常驻节点存在时，以上命令全部经通道复用该节点」。原实现让用户先 `stop`
再生成——而那恰恰会让新签的邀请落到临时节点上，撞回缺陷 1。
改为经通道让常驻节点签发（`PairGenerate` / `PairAccept` 两个动词）。

### 3. `inbox` 直连数据库会撞锁

`migration` 的连接**不设 `journal_mode`**，走 SQLite 的 `delete` 模式——写事务会阻塞所有读，
而常驻节点接收文件时一直在写。原实现的 `inbox` 一律直连库，在那种时刻会以
`database is locked` 失败。

改为：**有常驻节点走通道，没有才直连**（那时没有并发写者，直连既安全又不必为看一眼收件箱
起一个 P2P 节点）。通道刚断开的竞态也兜住了——回落直连而不是报错。

### 4. 非法邀请串被归为「对端不可达」

`pair` 把 `pair_with_invite` 的所有失败都映射成对端不可达（退出码 4）。但「串抄错了」与
「对方连不上」是两回事：前者要改参数重来，后者要等对方上线再试——而 spec 明确要求退出码
区分失败原因。

改为**先在本地解码一次**再交给节点。副作用是形似但损坏的邀请不会再白起一个临时节点去连。

实测：完全非邀请串与「形似但截断」的串现在都返回 2，且消息分别指出「未找到邀请链接前缀」
与「格式无法解析」。

## 8.7 / 9.1 / 9.2：核心验收已本地达成，只差「真的发出去」

原以为这三项非发布不可。实际把 dist 跑起来之后，**除了「发到公共 registry」这一步，
其余都能在本地验证**——而那一步验证的是 npm/homebrew 的接收方，不是本仓的正确性。

### 8.7 每个受支持平台都有产物 —— ✅ 本地已验证

`dist build --artifacts=all` 退出码 0，六个平台的归档与校验和齐全：

| target | 归档 | 校验和 |
|---|---|---|
| aarch64-apple-darwin | ✓ | ✓ |
| x86_64-apple-darwin | ✓ | ✓ |
| aarch64-unknown-linux-gnu | ✓ | ✓ |
| x86_64-unknown-linux-gnu | ✓ | ✓ |
| x86_64-unknown-linux-musl | ✓ | ✓ |
| x86_64-pc-windows-msvc | ✓ | ✓ |

外加一轮独立的逐平台交叉编译（绕开 dist，直接 `cargo zigbuild` / `cargo xwin`）：
**6 成功 0 失败**，`file` 确认每个二进制的架构正确（Mach-O arm64 / x86_64、ELF aarch64 /
x86-64、PE）。

⚠️ 本地跑 `dist build` 会打印
`× unable to run linkage report for aarch64-unknown-linux-gnu on macos`
——**那是非致命警告**（退出码仍是 0，产物齐全）。CI 用 `matrix.runner` 按 target 分配
原生 runner，根本不会走到这条路径。为此在 macOS 上装了 zig + cargo-zigbuild + cargo-xwin。

### 9.2 npm 包 —— ✅ 结构与下载地址已验证

`swarmdrop-cli-npm-package/` 的 `package.json`：

- `name: "swarmdrop"`（无 scope，如决定）
- `bin: { "swarmdrop": "run.js" }`
- `supportedPlatforms` 覆盖全部目标，各自映射到对应归档名与二进制名
- `postinstall` 走 `install.js` 按平台下载

**下载地址与 tag namespace 对得上**——这是最容易错的一处：不带 tag 构建时是
`releases/download/v0.1.0`，而带上真实 tag 后为
`releases/download/cli/v0.1.0`（含 namespace 段）。installer 脚本同样。

### 9.1 干净环境 —— ✅ 已验证（未走 installer 下载）

解出 aarch64 归档，在**全新 HOME、清空所有环境变量、PATH 只含产物目录**下：
`--version` 正常 · 起节点成功（20 个监听地址）· 生成邀请成功 ·
数据落在 `~/Library/Application Support/com.yexiyue.swarmdrop-cli/`（与桌面端区分开）。

### 真正剩下的

只有**推 tag 触发发布**本身，以及它之后才能做的两件事：经 installer / npm 真实安装一次、
以及需要第二台设备的 `send`。发布是对外且不可逆的操作（npm 版本号发出去不能重用），
按 2026-08-19 的决定推迟。

发版前要确认的三件本仓之外的事：

1. `NPM_TOKEN` 仓库 secret 已配置且有发布权限
2. `HOMEBREW_TAP_TOKEN` 已配置且对 `swarm-apps/homebrew-tap` 有写权限（该仓库已存在）
3. `swarmdrop` 这个 npm 名字在发布那一刻仍未被他人占用（现在是空的）

确认后：

```sh
./scripts/release-cli.sh --push
```

想先小成本验证整条流水线的话，手打一个 `-rc.1` 后缀的 tag——dist 会标成 prerelease。

> ⚠️ **更正（2026-08-19，实际发布后）**：这里原本写的是
> `git tag cli/v0.1.0 && git push origin cli/v0.1.0`，**那个 tag 形式是错的**，
> 0.1.0 就是照它发的。少了包名段会让 dist 判定为「整个 workspace 统一发布」，
> release notes 于是取**仓库根**的 `CHANGELOG.md`（桌面版本线）而不是
> `crates/cli/CHANGELOG.md`，且不报错。正确形式是 `cli/swarmdrop-cli-v<版本>`，
> 由 `scripts/release-cli.sh` 构造并校验。完整因果见
> [`cli-host.md`](../../../dev-notes/knowledge/cli-host.md) 的
> 「tag 形式决定 release notes 取哪份 CHANGELOG」。
