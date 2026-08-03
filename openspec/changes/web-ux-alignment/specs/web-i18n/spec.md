## ADDED Requirements

### Requirement: Web 应用区文案全量走 i18n

`docs/app/app` 下面向用户的文案 SHALL 经 i18n 运行时输出，SHALL NOT 硬编码任何自然语言字符串。

覆盖范围 SHALL 包含：正文与标题、按钮与菜单项、空态与教学文案、错误与状态提示、
无障碍属性（`aria-label` / `title` / `alt`）。

机器值 SHALL NOT 进入翻译目录：PeerId、multiaddr、哈希、文件名、原始字节数等。

#### Scenario: 组件内不残留硬编码文案

- **WHEN** 扫描应用区源码中面向用户的字符串字面量
- **THEN** 除机器值与内部标识外 SHALL 全部经 i18n 宏包裹

#### Scenario: 无障碍属性同样被翻译

- **WHEN** 用户在非中文 locale 下用读屏软件访问设备卡片的操作入口
- **THEN** 其无障碍名称 SHALL 为当前 locale 的译文

### Requirement: locale 集合与桌面端一致

Web 应用区 SHALL 支持 `zh`（源 locale）、`zh-TW`、`en` 三个 locale，与桌面前端保持同一集合。

翻译目录 SHALL 独立于桌面目录存放；两者 SHALL NOT 互相 import 或共用同一份 catalog 文件。

#### Scenario: 三个 locale 均可用

- **WHEN** 用户依次切换到 zh、zh-TW、en
- **THEN** 应用区 SHALL 分别以对应语言呈现，SHALL NOT 出现回退到 msgid 的裸串

#### Scenario: 新增文案可被提取

- **WHEN** 在应用区新增一条经 i18n 宏包裹的文案并运行提取命令
- **THEN** 该文案 SHALL 出现在三个 locale 的目录中待翻译

### Requirement: 静态导出下的 locale 选择与持久化

locale 选择 SHALL 在客户端完成，SHALL NOT 依赖服务端协商或按 locale 预生成多套路由——
应用区是静态导出且路由集合固定。

首次访问 SHALL 依据浏览器语言偏好选择最接近的受支持 locale，无匹配时回退到源 locale；
用户显式选择后 SHALL 持久化并在后续访问中优先于浏览器偏好。

#### Scenario: 首访依浏览器偏好

- **WHEN** 浏览器首选语言为英语的用户首次访问应用区
- **THEN** 界面 SHALL 以英语呈现

#### Scenario: 显式选择被记住

- **WHEN** 用户显式切换到 zh-TW 后关闭并重新打开页面
- **THEN** 界面 SHALL 仍为 zh-TW，SHALL NOT 回退到浏览器偏好

#### Scenario: 不受支持的偏好回退

- **WHEN** 浏览器首选语言不在受支持集合内且用户未做过显式选择
- **THEN** 界面 SHALL 以源 locale 呈现

#### Scenario: 路由数量不随 locale 增长

- **WHEN** 执行生产构建
- **THEN** 应用区导出的路由 SHALL 与单 locale 时相同，SHALL NOT 出现按 locale 复制的路由

### Requirement: i18n 范围止于应用区

本能力 SHALL 只覆盖 `docs/app/app`。文档正文、营销页与站点外壳 SHALL NOT 被纳入，
其现有呈现 SHALL NOT 因本次接入而改变。

#### Scenario: 文档区不受影响

- **WHEN** i18n 接入后访问文档正文与营销页
- **THEN** 其内容与呈现 SHALL 与接入前一致
