# Changelog

`swarmdrop` 命令行工具的变更记录。

> 这条版本线独立于桌面端（tag `v*`）与移动端（tag `mobile-v*`），tag 形如
> `cli/swarmdrop-cli-v0.1.1`（0.1.0 用的是旧形式 `cli/v0.1.0`，见 dist-workspace.toml）。
> 仓库根目录的 `CHANGELOG.md` 记的是桌面端，与本文件无关。

## [未发布]

### 新增

- **`swarmdrop transfer watch`：实时盯着正在进行的传输。** 每条未结束的会话一行进度条，
  正在传的那条显示速率与剩余时间，等待确认 / 已暂停的只显示状态（给它们画速率是在报告
  一个假事实）。面板上带热键：`p` 暂停、`r` 恢复、`c` 取消、`q` 退出——按下去弹的就是
  下面三条命令那套多选菜单。

  没有常驻节点时它照样能开：列出的是等着续传的那几条。`--json` 下退化成每秒一行的
  NDJSON 快照流，供脚本消费。

- **`swarmdrop transfer pause / resume / cancel`。** 不带标识时列出**此刻能做这个动作**的
  会话让你勾选（可多选）：暂停只列正在传的，恢复只列可续传的，取消只列尚未结束的——
  菜单里出现的每一条都当真做得成。

  三条都要求常驻节点在跑，且**绝不为它们起临时节点**：它们作用的是常驻节点内存里的活
  传输，临时节点里空空如也。没有节点时立即报错并指出先 `swarmdrop start`。

### 修复

- **传输列表与详情不再打印生的英文状态**（`active` / `suspended`），改为与桌面端同一套
  中文措辞（「传输中」「已暂停」「对方暂停」「已中断」「未及时处理」…）。

### 变更

- **`invite create` 不再在终端画二维码**，改为只输出邀请链接并提示「在浏览器打开它，
  页面会显示二维码」——链接指向的 `/p/` 落地页按需渲染。**`--no-qr` 开关随之删除。**

  判据是尺寸且没有可调空间：邀请的体积由签名与公钥主导而非地址，裁到 `fit` 的下界
  （只剩一条地址）仍是 69 模块；半块字符已经是长宽比正确的最密形态，于是终端占用恒为
  **69 列 × 35 行**起，标准 80×24 终端一屏半装不下，而命令还要在码下面继续输出。
  换成 2×2 象限块能把宽度减半，但会把码压成 2:1 的竖长条，扫码器直接读不出。

  扫码这条路径没有断，只是换了承载：浏览器没有终端的宽高比与行数约束。

### 修复

- **Windows 上 `swarmdrop start` 直接起不来**（报「本地通道路径不可用: not a named pipe
  path」）。本地通道此前一律取数据目录下的 `swarmdrop.sock`，而 Windows 的命名管道不在
  文件系统里——`interprocess` 只接受 `\\.\pipe\` 开头的名字。现在按平台分叉：Unix 仍是
  数据目录下的域套接字（因而继续受目录 0700 保护），Windows 用一个从数据目录派生的管道名
  `\\.\pipe\swarmdrop-cli-<hash>`，`--data-dir` 的多个实例互不串台。
  ⚠️ 已知缺口：Windows 的通道不在数据目录里，那道 0700 保护不到它，而管道默认 DACL 的
  实际放行范围本仓尚未实测。

### 改进

- **所有需要「目标」的命令都能不带参数运行**，缺的那个由交互补出来：

  | 命令 | 不带参数时 |
  |---|---|
  | `send` | 先列出已配对设备让你选，再逐行问要发什么（可拖进终端 · Tab 补全） |
  | `invite use` | 问你要邀请链接 |
  | `invite revoke` | 列出未过期邀请，**勾选若干张** |
  | `device forget` | 列出已配对设备，**勾选若干台** |
  | `inbox show` / `export` | 列出收件箱让你选；`export` 还会问目标目录（默认当前目录） |
  | `transfer show` | 列出传输记录让你选 |

- **撤销邀请与解除配对支持一次多个**，参数侧同样收多个（`invite revoke a b c`）——
  此前只能一次一个，脚本得把同一条命令循环敲 N 遍。
- **交互提示改用彩色主题**，长列表自动翻页。
- 发送的路径输入认得 shell 转义与引号，所以把几个文件一起**拖进终端**就能用；
  `~` 会展开。

  这些行为在**不可交互的环境里一个都不变**：`--json` / `--no-input` / 管道下缺参数仍然
  立即以用法错误退出（退出码 2），绝不停下来等一个不会到来的回答。

### 破坏性变更（`--json`）

两条命令的结构化输出因为「一次可处理多个」而改了形状。**只影响解析 `--json` 的脚本**，
人类可读输出与退出码不变：

| 命令 | 0.1.0 | 现在 |
|---|---|---|
| `invite revoke <id>` | `{"event":"inviteRevoked","id":…,"persisted":…}` | `{"event":"invitesRevoked","ids":[…],"revoked":N,"persisted":…}` |
| `invite revoke --all` | `{"event":"invitesRevoked",…}` | `{"event":"allInvitesRevoked",…}` |
| `device forget <dev>` | `{"event":"deviceForgotten","peerId":…,"name":…,"remaining":…}` | `{"event":"devicesForgotten","devices":[{"peerId":…,"name":…}],"remaining":…}` |

⚠️ `invite revoke --all` 的 `event` 一并改名，是因为逐张撤销现在占用了 `invitesRevoked`
——两者是不同的动作（撤指定的几张 vs 连这一瞬新签发的也作废），共用一个名字会逼脚本
去探测 `ids` 字段在不在才能分辨。

## [0.1.0] - 2026-08-19

首个正式版本。

### 是什么

SwarmDrop 的命令行宿主——桌面 / 移动 / Web 之外的第四个宿主，与前三者共用同一套 P2P 内核
与配对协议。无账号、无中心服务器，设备间端到端加密传输，局域网与跨网都走同一条路径。

### 命令面

```
swarmdrop start / stop / status      常驻节点的生命周期
swarmdrop send <FILES> --to <设备>   发送文件或目录
swarmdrop invite create | use | list | revoke
swarmdrop device  list | forget
swarmdrop inbox   list | show | export
swarmdrop transfer list | show
```

### 为脚本与 agent 设计

- **退出码区分失败原因**，无需解析文本：用法错误 2、节点不可用 3、对端不可达 4、
  传输失败 5、配对被拒 6、被中止 130。同一件事在「有常驻节点」与「无常驻节点」两条路径上
  给出同一个退出码。
- **`--json`** 让每条命令输出结构化结果，且 stdout 只有最终结果——进度、诊断、日志一律走
  stderr。
- **`--no-input`** 显式关闭交互，用于服务单元与 CI；缺参数时不会挂起等待，而是以用法错误退出。
- **只读命令不启动节点**：`device list` / `invite list` / `inbox list` / `transfer list` 等
  直接读本机记录，毫秒级返回。邀请泄露后的撤销尤其不该要求先启动一个可能失败的节点。

### 交互补全

参数给全就直接执行；缺参数且有终端时列出候选让你选（撤销哪张邀请、解除哪台设备）；
缺参数又没有终端则立即以用法错误退出——不猜默认值，因为这两个操作都不可逆。

### 与图形界面并存

命令行宿主使用**独立的设备身份**与数据目录，同一台机器上与桌面端同时运行时互不干扰，
在对方的设备列表里显示为两台设备（名字带 ` (cli)` 后缀）。

### 安装

```bash
# macOS / Linux
curl -fsSL https://github.com/swarm-apps/SwarmDrop/releases/download/cli/v0.1.0/swarmdrop-cli-installer.sh | sh

# Homebrew
brew install swarm-apps/tap/swarmdrop

# npm（给 agent harness 用：按平台解析到对应二进制）
npx swarmdrop --help
```

支持 macOS（Apple Silicon / Intel）、Linux（x86_64 / aarch64，gnu 与 musl）、Windows x86_64。
