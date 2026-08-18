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

## 未完成项与原因

- **8.7 试发预发布版本**、**9.1 干净机器验收**、**9.2 npm 渠道验收**
  这三项都需要**推送 git tag 触发真实发布**——一次对外操作，且会往 npm 与 homebrew tap 写入。

  **本地已验证到能验证的极限**：
  - `dist plan` 退出码 0，六个目标的归档与校验和齐备
  - `dist build --target aarch64-apple-darwin` 构建成功（本机唯一能构建的目标；
    其余五个需要 `cargo-xwin` / `cargo-zigbuild`，那是 CI 里才有的交叉编译工具链）
  - 解出的产物 `swarmdrop --version` 正常、能启动节点。21 MB 二进制 / 5.9 MB 归档
  - ⚠️ 过程中发现并修掉了一个**CI 里同样会炸**的问题：缺 `[profile.dist]`，
    `dist build` 会以「profile `dist` is not defined」失败

### 发版：已决定推迟（2026-08-19）

代码与配置已提交并推送到 `develop`（`c7acff46` / `fe2c00e7` / `b5c5f082`）。
**发版本身明确推迟**，不是遗漏。

npm 包名已定为**无 scope 的 `swarmdrop`**：scoped 包要求那个 npm 组织已存在且发布者有
权限，而组织是否存在无法从外部查证——押错会卡在 CI 的 publish 步骤，且 npm 已发布的
版本号不可重用。无 scope 名不依赖任何组织，`npx swarmdrop` 也更短。

**发版前要确认的三件事**（都在本仓之外，我查不到）：

1. **`NPM_TOKEN`** 仓库 secret 已配置且有发布权限
2. **`HOMEBREW_TAP_TOKEN`** 已配置且对 `swarm-apps/homebrew-tap` 有写权限（该仓库已存在）
3. `swarmdrop` 这个 npm 名字在发布那一刻仍未被他人占用（现在是空的）

确认后：

```sh
git tag cli/v0.1.0 && git push origin cli/v0.1.0
```

想先小成本验证整条流水线的话，用 `cli/v0.1.0-rc.1`——dist 会标成 prerelease。

- 标记为 🔵 的 6 条都需要**第二台已配对设备**（配对握手、发送、被动接收）。
  代码路径完整、单测覆盖各自的纯逻辑部分，但「两台设备真的传成了」这件事本机验证不了。
