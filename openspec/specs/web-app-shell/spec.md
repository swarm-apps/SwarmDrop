# web-app-shell Specification

## Purpose
TBD - created by archiving change web-ux-alignment. Update Purpose after archive.
## Requirements
### Requirement: Web 应用区以移动优先为布局基准

`docs/app/app` 下的所有视图 SHALL 以单栏、无 hover 依赖的窄屏形态为基线实现，宽屏为渐进增强。

任何交互 SHALL NOT 只能经 hover 触达；触摸目标 SHALL 不小于 44×44 CSS 像素。

#### Scenario: 窄屏下功能不缺失

- **WHEN** 在 375px 宽的视口打开应用区任一路由
- **THEN** 该路由的全部功能 SHALL 可达，SHALL NOT 出现横向滚动或被裁切的操作入口

#### Scenario: 仅 hover 可见的操作不被接受

- **WHEN** 某个操作入口只在指针 hover 时出现
- **THEN** 该实现 SHALL 视为不满足要求——它在触摸设备上不可达

### Requirement: 主从布局断点与桌面端统一为 920px

Web 应用区中需要「列表 + 详情」的视图（收件箱、传输）SHALL 使用 `(min-width: 920px)` 作为主从
双栏的断点，与桌面端 `MASTER_DETAIL_QUERY` 取同一个数。

窄于该断点时，详情 SHALL 占满，列表 SHALL 经抽屉或返回导航到达。SHALL NOT 各视图各写各的断点。

#### Scenario: 宽屏呈现双栏

- **WHEN** 视口宽度 ≥920px 且用户打开收件箱
- **THEN** 系统 SHALL 呈现左列表 + 右详情双栏

#### Scenario: 窄屏收成单栏

- **WHEN** 视口宽度 <920px 且用户打开收件箱
- **THEN** 详情 SHALL 占满可用宽度，列表 SHALL 经可展开的容器到达

#### Scenario: 断点在视图间一致

- **WHEN** 视口宽度在 920px 附近变化
- **THEN** 收件箱与传输 SHALL 同时切换形态，SHALL NOT 一个已切另一个未切

### Requirement: 组件底座为 shadcn/ui，token 经映射层接入 fumadocs

Web 应用区 SHALL 使用 shadcn/ui（`new-york` 风格）作为组件底座，而非手写原生元素。

`docs/app/global.css` SHALL 提供一层 token 映射，把 fumadocs 的 `--color-fd-*` 变量映射为 shadcn
所需的语义 token；SHALL NOT 在应用区自建第三套 token 体系，也 SHALL NOT 改动文档区的既有呈现。

品牌色 SHALL 与桌面 `src/index.css` 取同一组值。

#### Scenario: 应用区与文档区共存

- **WHEN** 接入 shadcn/ui 与 token 映射层后访问文档区任一页面
- **THEN** 文档区的呈现 SHALL 与接入前一致（明暗两种模式均如此）

#### Scenario: 组件跟随明暗模式

- **WHEN** 用户在站点内切换明暗模式
- **THEN** 应用区的 shadcn 组件 SHALL 随之切换，SHALL NOT 出现与文档区不一致的残留配色

#### Scenario: 品牌色三端同值

- **WHEN** 对比 Web 应用区与桌面端的品牌强调色
- **THEN** 两者 SHALL 为同一组值（明暗各一），SHALL NOT 各自维护

### Requirement: 类名合并工具支持条件类名

`docs` 的 `cn` 工具 SHALL 支持条件类名（对象 / 数组 / 假值形参）后再做 Tailwind 冲突合并。

#### Scenario: 条件类名被正确解析

- **WHEN** 以 `cn("base", { active: true, hidden: false }, undefined)` 形式调用
- **THEN** 结果 SHALL 含 `base` 与 `active`，SHALL NOT 含 `hidden`，也 SHALL NOT 出现 `[object Object]`

### Requirement: 运行时单例与静态导出约束在新形态下继续成立

重构后，节点 spawn、事件消费、状态轮询与 relay 意图 SHALL 仍然只在应用区 layout 挂载一次；
SHALL NOT 因视图拆分或组件下沉而在任一 page 中重复启动。

静态导出的既有限制 SHALL 继续满足：SHALL NOT 引入动态路由段，内部导航 SHALL 走框架的链接组件，
读取查询参数的组件 SHALL 包裹在 Suspense 边界内。

#### Scenario: 切换路由不重启节点

- **WHEN** 用户在应用区的五条路由间来回切换
- **THEN** 节点 SHALL 保持同一实例，事件 SHALL NOT 被重复消费

#### Scenario: 静态导出构建通过

- **WHEN** 重构完成后执行 docs 的生产构建
- **THEN** 构建 SHALL 成功，SHALL NOT 出现 CSR bailout 或动态段预生成失败

#### Scenario: 子路径部署下链接可用

- **WHEN** 站点部署在子路径下，用户点击应用区内的任一导航项
- **THEN** 导航 SHALL 成功，SHALL NOT 因缺少路径前缀而 404

### Requirement: 状态管理约束在重构后继续成立

应用区的 store selector SHALL 只返回原始值或 store 内的稳定引用，派生数组 / 对象 SHALL 放在
记忆化层；「内容未变」时 SHALL 返回原状态对象本身而非新建空对象。

#### Scenario: 门禁脚本通过

- **WHEN** 重构完成后运行仓库的 zustand 访问检查
- **THEN** 检查 SHALL 通过，应用区 SHALL NOT 出现在违规清单中

### Requirement: 导航定义保持单一事实源

应用区的路由、标题、描述、图标与徽标 SHALL 继续由单一导航定义模块派生；跨页链接 SHALL NOT
在组件内手拼路径字面量。

#### Scenario: 改一处路由段全站生效

- **WHEN** 修改导航定义中某条路由的路径
- **THEN** 侧边栏、底部导航、页头与全部跨页链接 SHALL 同步生效，SHALL NOT 留下失效的硬编码路径

