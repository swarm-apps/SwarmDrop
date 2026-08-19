# Changelog

`swarmdrop` 命令行工具的变更记录。

> 这条版本线独立于桌面端（tag `v*`）与移动端（tag `mobile-v*`），tag 形如
> `cli/swarmdrop-cli-v0.1.1`（0.1.0 用的是旧形式 `cli/v0.1.0`，见 dist-workspace.toml）。
> 仓库根目录的 `CHANGELOG.md` 记的是桌面端，与本文件无关。

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
