## Purpose

把 SwarmDrop 的发送、设备、收件箱与传输能力以 MCP stdio server 的形态暴露给任何 agent
harness，使「能执行一条命令」成为接入 SwarmDrop 的唯一前提。它与桌面端进程内的
streamable-http server（`mcp-server`）语义对齐但宿主与传输完全不同，且不要求安装桌面端。

## ADDED Requirements

### Requirement: stdio 传输与 stdout 纯净

系统 SHALL 提供 `swarmdrop mcp` 命令，在标准输入输出上实现 MCP stdio 传输。
stdout SHALL 只承载 MCP 协议帧；日志、进度、诊断与任何面向人的文案 SHALL 一律走 stderr。

这条是硬约束而非风格：stdout 混入一个字节的非协议内容，宿主的解析器就会失败，而失败
形态是「server 启动了但一个工具都不可见」——与「没装 SwarmDrop」无法区分。

#### Scenario: 协议帧与日志分流

- **WHEN** `swarmdrop mcp` 运行期间产生了节点状态变化的日志
- **THEN** 该日志 SHALL 写入 stderr，stdout SHALL 只出现 MCP 协议帧

#### Scenario: 宿主经 stdio 完成握手

- **WHEN** 宿主以 `command: swarmdrop` / `args: ["mcp"]` 启动本 server 并发起 MCP 初始化
- **THEN** 系统 SHALL 在 stdout 回应握手并公布其工具清单

### Requirement: 工具面

系统 SHALL 至少暴露四类工具：**发送**（向已配对设备发送文件或文本）、**设备**（列出已配对
设备及其在线状态）、**收件箱**（列出、检索、取条目详情与条目内文件的本地路径）、
**传输**（列出会话、查询单个会话状态、暂停 / 恢复 / 取消）。

工具的语义 SHALL 与桌面端同名工具保持一致，使同一个 agent 在两种宿主下的行为可预期。

系统 SHALL NOT 暴露「代收入站传输」类工具。CLI 常驻节点对来自已配对设备的入站内容本就
自行确认，模型不需要这项能力；而把它暴露出去等于把「收不收」这个决定从人手里移交给模型。

#### Scenario: 发送到已配对设备

- **WHEN** 模型调用发送工具并给出目标设备与一个存在的文件路径
- **THEN** 系统 SHALL 发起传输并返回可用于后续查询的会话标识

#### Scenario: 目标设备不存在

- **WHEN** 模型调用发送工具，给出的目标既不匹配任何已配对设备名也不匹配任何节点标识
- **THEN** 系统 SHALL 返回明确的工具错误，说明目标无法解析，且 SHALL NOT 发起任何传输

#### Scenario: 取收件箱条目的本地路径

- **WHEN** 模型先检索收件箱定位到一个条目，再请求该条目内某个文件的本地路径
- **THEN** 系统 SHALL 返回该文件在本机的真实路径；文件缺失或不可达时 SHALL 明确报告，
  且 SHALL NOT 返回一个无效路径

### Requirement: 节点接入与生命周期

`swarmdrop mcp` SHALL 按「有常驻节点就复用、没有就自持一个」接入网络，自持的节点
SHALL 与 server 同生命周期——server 退出时随之关停。

server SHALL NOT 为每次工具调用反复启停节点：那会让每次发送都要重新连引导节点并做 NAT
探测，把一次本该是秒级的调用拖成数秒。

#### Scenario: 复用常驻节点

- **WHEN** 本机已有 `swarmdrop start` 的常驻节点，此时启动 `swarmdrop mcp`
- **THEN** 系统 SHALL 经本地通道复用该节点，且 SHALL NOT 另起一个节点

#### Scenario: 自持节点

- **WHEN** 本机没有常驻节点，此时启动 `swarmdrop mcp`
- **THEN** 系统 SHALL 自行启动一个节点并持有到 server 退出

#### Scenario: 宿主关闭 stdin

- **WHEN** 宿主关闭 stdin 或终止本进程
- **THEN** 系统 SHALL 关停自持的节点并释放单实例锁后退出

### Requirement: 调用方是程序

`swarmdrop mcp` SHALL 等同于声明了禁止交互：任何缺参数、需确认或需选择的路径
SHALL NOT 弹出交互提示，而是以明确的工具错误返回。

#### Scenario: 需要选择目标时不弹提示

- **WHEN** 某次工具调用缺少必需的目标参数
- **THEN** 系统 SHALL 直接返回参数错误，且 SHALL NOT 尝试从终端读取输入

### Requirement: 配对窗口不因 MCP 而打开

`swarmdrop mcp` SHALL NOT 接受入站配对请求，无论对方是否出示了有效邀请。

邀请会泄露且是一次性的，被抢走那次就消耗掉了凭证；配对窗口只应在人正在等待时打开
（即 `swarmdrop invite create` 运行期间）。一个长驻的 MCP server 后面没有人在看，
它不构成那个窗口。

#### Scenario: MCP server 运行期间收到入站配对

- **WHEN** `swarmdrop mcp` 正在运行且没有 `swarmdrop invite create` 在等待，此时有对端
  出示有效邀请请求配对
- **THEN** 系统 SHALL 拒绝该请求，且 SHALL NOT 消费该邀请凭证
