## Purpose

规定命令行宿主如何被打包与分发，使其能在未安装过 SwarmDrop 的机器上一条命令装好，
并可被外部程序（agent harness 的包依赖）以平台二进制的形式引入。

## ADDED Requirements

### Requirement: 独立的版本线

CLI SHALL 使用一条与桌面、移动互不重叠的版本线与发布标签命名空间。

任一条版本线的发布标签 MUST NOT 触发其余版本线的发布流水线。

#### Scenario: CLI 发布不触发桌面发布

- **WHEN** 推送一个 CLI 版本标签
- **THEN** 仅 CLI 的发布流水线运行，桌面与移动的发布流水线不被触发

#### Scenario: 桌面发布不触发 CLI 发布

- **WHEN** 推送一个桌面版本标签
- **THEN** CLI 的发布流水线不被触发

#### Scenario: 标签形态被验证

- **WHEN** 建立发布流水线
- **THEN** 所选标签形态能被分发工具正确解析出版本号，并在文档中记录该形态

### Requirement: 发布流水线彼此隔离

CLI 的发布流水线定义 SHALL 独立于既有的桌面发布流水线定义，
生成或再生成 CLI 流水线 MUST NOT 覆盖或修改既有的桌面发布流水线。

#### Scenario: 再生成不破坏既有流水线

- **WHEN** 重新生成 CLI 的发布流水线定义
- **THEN** 既有的桌面发布流水线定义内容不变

### Requirement: 多渠道安装

CLI SHALL 通过以下渠道分发：类 Unix 的一键安装脚本、Windows 的一键安装脚本、
npm 包、Homebrew formula。

Homebrew formula SHALL 发布到组织既有的 tap，不新建 tap。

#### Scenario: 未装过的机器一条命令装好

- **WHEN** 在一台从未安装过 SwarmDrop 的机器上执行任一渠道的安装命令
- **THEN** 命令行程序可用，无需另行安装图形界面宿主

#### Scenario: npm 包可作为依赖被引入

- **WHEN** 另一个 npm 包把 CLI 声明为依赖并按当前平台解析
- **THEN** 安装后可执行到对应平台的二进制

### Requirement: 平台矩阵

发布 SHALL 覆盖 macOS（Apple Silicon 与 Intel）、Linux（x86_64 与 aarch64）
与 Windows（x86_64）。

#### Scenario: 每个受支持平台都有产物

- **WHEN** 一次发布完成
- **THEN** 上述每个平台都存在可下载的构建产物

### Requirement: 更新按安装渠道分派

> **2026-08-20 修订。** 本条此前是「CLI MUST NOT 内建自更新程序」，理由是「自更新与包
> 管理器的更新路径并存会产生版本来源不一致」。那条顾虑仍然成立，但它的解法是**先认出
> 渠道**，而不是不做自更新——原条款把「不与包管理器争事实源」误当成了「不提供更新命令」，
> 于是最需要一句「请用 brew 升级」的那批用户，恰恰得不到任何提示。

CLI SHALL 先判定安装渠道，再决定更新方式：由 dist 的安装脚本装的（有 install receipt
**且它指向正在运行的这个可执行文件**）SHALL 就地更新；由包管理器装的 SHALL 转交给该包
管理器并**以成功退出**；认不出渠道时 SHALL 只给通用指引，MUST NOT 猜测具体命令。

CLI MUST NOT 在任何情况下改写不由它管理的安装位置——那正是「版本来源不一致」的实际形态。

节点运行期间 MUST NOT 就地更新：更新会覆盖本程序的可执行文件，而 Unix 上覆盖是允许的
（替换目录项，运行中的进程仍持旧 inode），失败形态因此是**静默**的——命令报「已更新」，
跑着的节点仍是旧代码。

#### Scenario: 安装脚本装的那份

- **WHEN** 用户执行更新，且 install receipt 指向正在运行的可执行文件
- **THEN** 就地下载并替换，报告新旧版本与安装位置

#### Scenario: 包管理器装的那份

- **WHEN** 用户执行更新，而这份程序由 Homebrew 或 npm 安装
- **THEN** 命令给出该包管理器对应的升级命令并**以成功退出**
- **AND** 系统 MUST NOT 改写任何文件

#### Scenario: 认不出渠道

- **WHEN** 渠道无法判定（源码构建、`cargo install`、手动解压）
- **THEN** 命令只给出通用指引与发布页链接，不猜测具体命令

#### Scenario: 节点正在运行

- **WHEN** 用户在节点运行期间执行更新
- **THEN** 命令拒绝执行并指出应先停止节点，退出码与「节点不可用」区分开
