## Why

`agent-harness-integration` 把 SwarmDrop 接进了 DeepSeek Harness，但只接了**给模型用的那一半**
（MCP 工具 + 事件订阅）。任务 10.7「`ctx.slots` 设备面板 + 设置卡」当时明确不做，于是**给人用的
那一半是空的**：dsh 里没有任何地方显示节点在不在跑、网络通不通、配了哪些设备，配对要用户切到
终端敲命令。2026-08-21 的实机反馈就是这三条。

侧栏面板（`sidebar.footer.action`）已在插件仓补上，随即暴露出两个更深的问题——一个在本仓：

1. **订阅会把离线的已配对设备抹掉。** `swarmdrop watch` 把内核的 `CoreEvent::DevicesChanged`
   直接翻译成订阅面的同名事件，但内核那份是 `DeviceFilter::All`（本次运行**被观测到**的 peer），
   而订阅面的契约是**全量已配对设备表**。一台已配对却本次运行没上过线的设备不在前者里，于是
   实测形态是「基线报 1 台 → 下一条网络事件一到变 0 台」，看起来跟刚被解除配对一模一样。
   现有 spec 只写了「SHALL NOT 包含未配对的设备」——是**纯度**判据，而这里破的是**完整性**。
2. （仓外，已修）订阅进程死掉后插件永不重连，镜像冻结却继续自信播报。

而要把设置搬进 dsh 的设置页（`settings.section`），挡在前面的是一个硬事实：**CLI 一个配置动词
都没有**。设备名已经持久化了却没有命令改它；接收落点只认环境变量、不持久化；引导节点清单是
编译期常量。桌面 / 移动 / Web 三端都能改的东西，第四个宿主一样都改不了——`device-naming` 与
`bootstrap-node-settings` 两份 spec 至今写的都是「三端」。

## What Changes

- **`crates/cli` 新增两个名词**：`config`（标量设置——**设备名**与**接收落点**，动词
  `list` / `get` / `set` / `unset`）与 `bootstrap`（**自定义引导节点**这个集合，动词
  `list` / `add` / `remove`）。分两个名词而不是把清单塞进 `config set`，是因为 custom 与
  removed 两集合模型下的用户意图本来就是「加一条」「撤一条」，而 `set` 表达的是整值替换——
  用 `set` 表达它等于回到「持久化合并后的最终清单」那个已知的坑（见 `bootstrap-node-settings`）。
  两者都只读写本机记录，因此 MUST NOT 启动节点；有节点在跑时改名与增删引导节点 SHALL 即时
  生效（分别复用 `live-device-rename` 与既有的 infra 意图接口，都不重启节点）。
- **接收落点获得持久化配置**，且 `SWARMDROP_RECEIVE_DIR` **仍然优先**：命令行宿主常跑在脚本与
  服务单元里，环境变量是那些地方的自然覆盖手段，加了配置文件不等于要把它降级。
- **引导节点清单从编译期常量变为「内置 + 用户增删」**，与另外三端同构：持久化 custom 与
  removed **两个集合**而非合并后的最终清单（合并清单会在版本更新换内置地址时把老用户永久
  压在旧地址上）。持久化位置是**数据目录**，不是前端偏好存储——命令行宿主没有后者。
- **修正订阅面的设备表**（已实现）：设备表 SHALL 只有一个产出点——向节点现取
  `DeviceFilter::Paired`，与基线同一口径；内核 `DevicesChanged` 降级为「表可能变了」的信号，
  其载荷 SHALL NOT 被直接转发。
- **仓外（`dsh-swarmdrop`）**：`settings.section` 一整页（节点与网络全量地址 / 设备 / 邀请清单
  与撤销 / 收件箱完整列表与导出 / 传输记录 / 关于），侧栏面板的按控件 busy 与调用超时，订阅
  监督重连，品牌图标。分两批：先做 CLI 已经给得出的，再随本变更的 config 动词补上设置行。

**非目标**：开机自启（`start.rs` 已明确交给服务管理器，CLI 不该自己造一套）；MCP 端口与
主题 / 语言（前者是桌面 HTTP server 的概念，CLI 的 MCP 是 stdio；后者命令行宿主没有）。

## Capabilities

### New Capabilities

- `cli-config-surface`: 命令行宿主的配置面——哪些设置可读可写、命令形状、环境变量与持久化
  配置的优先级、生效时机（是否需要重启节点）、以及 `--json` 读面的形状（dsh 插件的设置页
  与其它程序化消费方靠它显示当前值）。引导节点集合的**内容**判据仍归
  `bootstrap-node-settings`，本能力只管它在命令面上的形状与生效语义。

### Modified Capabilities

- `device-naming`: 「三端都可查看与修改设备名」扩为**四端**——命令行宿主 SHALL 同样提供读写
  能力，且 SHALL NOT 依赖节点处于运行状态。
- `bootstrap-node-settings`: 自定义引导节点扩到命令行宿主，并明确它的持久化位置是数据目录
  （该 spec 现有条文把持久化位置写死在三端各自的前端偏好存储里）。
- `cli-event-stream`: 设备表增加**完整性**判据与**来源**判据——现有条文只约束了纯度
  （不含未配对设备），而实际缺陷是遗漏（离线且本次运行未被观测到的已配对设备被整台抹掉）。

## Impact

- **`crates/cli`**：新增 `cmd/config.rs` 与 `cmd/bootstrap.rs`，以及对应的 IPC 动词
  （有节点时改名与增删引导节点要经它即时生效）；
  `adapter/receive.rs` 的落点解析接入持久化配置；`runtime/bootstrap_nodes.rs` 由常量改为
  「内置 + 增删」；`runtime/watch/{event,serve}.rs` 的设备表来源（**已实现**）。
- **配置持久化**：数据目录下的设备配置文件已存在（`JsonFileDeviceConfig`），落点与引导节点
  需要各自的持久化位置；是否复用同一个文件由 design 决定。
- **`crates/core` / `crates/host`**：预计不改。改名走既有的 `live-device-rename` 编排，引导
  节点走既有的 `NetworkRuntimeConfig` 与 infra 意图接口。
- **仓外 `/Volumes/yexiyue/dsh-swarmdrop`**：插件的浏览器半边与 Host 半边，见 tasks 第 5 节。
- **兼容性**：`swarmdrop config` 是纯新增，不改任何现有命令的形状与退出码。引导节点由常量
  变为可配后，未做过任何增删的用户 SHALL 仍然拿到内置清单的最新版本。
