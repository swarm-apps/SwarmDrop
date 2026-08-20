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

### Requirement: 不内建自更新

CLI MUST NOT 内建自更新程序；更新 SHALL 由安装它的渠道负责。

自更新与包管理器的更新路径并存会产生版本来源不一致。

#### Scenario: 通过安装渠道更新

- **WHEN** 用户希望升级 CLI
- **THEN** 使用其原本的安装渠道完成升级，程序自身不提供更新命令
