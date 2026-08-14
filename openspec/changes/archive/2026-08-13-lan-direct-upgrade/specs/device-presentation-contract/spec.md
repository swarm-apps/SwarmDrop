## ADDED Requirements

### Requirement: 连接徽标必须可展开为链路详情

三端的连接徽标 SHALL 提供通往链路详情的入口，展示传输协议、远端 multiaddr、以及经中继时的
中继身份。详情 SHALL 默认收起——对普通用户它是噪音，对排障的人它是全部证据。

呈现形态是**允许的分叉**：桌面与 Web 用悬浮层（两端有指针，浮层不挡内容），移动端就地展开
（触摸设备上的浮层会盖住半屏，而移动端那一屏本就是详情页）。

传输协议名 SHALL NOT 被翻译——用户拿它去搜索、比对日志、贴进 issue。

远端地址 SHALL 可复制且 SHALL NOT 在详情中被截断：截断的 multiaddr 贴进 issue 是废的。

#### Scenario: 链路详情缺席时徽标退化为静态徽标

- **WHEN** `connectionDetails` 为空（内核尚未报告连接地址）
- **THEN** 徽标 SHALL 渲染为不可交互的普通徽标，**SHALL NOT** 提供一个点开是空的入口

#### Scenario: 传输未知时照实呈现

- **WHEN** 链路详情的传输协议为空
- **THEN** 呈现层 SHALL 显示「未知」，**SHALL NOT** 显示某个猜测的默认值

### Requirement: 连接徽标的出现条件只取决于连接方式

连接徽标 SHALL 在设备在线且 `connection` 已知时出现，**SHALL NOT** 把 `latency` 并入出现条件。
延迟要等第一次 ping 采样（30 秒间隔），把它并进条件等于「刚连上的半分钟里连接方式不显示」。

延迟已知时 SHALL 显示在徽标内，未知时 SHALL 省略该部分而徽标其余部分照常。

#### Scenario: 刚建立连接、尚无延迟采样

- **WHEN** 设备刚连上，`connection` 已知而 `latency` 仍为空
- **THEN** 连接徽标 SHALL 已经出现（不带延迟）
