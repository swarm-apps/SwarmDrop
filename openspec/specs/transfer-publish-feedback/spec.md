# transfer-publish-feedback Specification

## Purpose
TBD - created by archiving change transfer-eta-and-publish-feedback. Update Purpose after archive.
## Requirements
### Requirement: 发布阶段对用户可见

接收侧把收齐的文件从暂存位置发布到用户目标位置的这段时间，SHALL 向前端广播文件级的发布
事件，使 UI 能在「字节已收完」与「文件已落地」之间给出解释。

事件 SHALL 是文件级而非会话级——发布是「收齐即发布」，一个会话里会发生多次，散布在整条
传输过程中，而不是末尾一次。

事件 SHALL NOT 过网：它只走本地 UI 链路，SHALL NOT 引入新的 wire 帧，也 SHALL NOT 触发
数据面协议版本变更。

#### Scenario: 文件收齐，开始发布

- **WHEN** 某个文件的所有数据块已落盘，发布即将开始
- **THEN** 系统 SHALL 广播一条携带 `session_id`、`file_id`、文件名、相对路径与总字节数的
  发布开始事件
- **AND** 该事件 SHALL 在发布动作之前发出

#### Scenario: 文件已落地

- **WHEN** 发布成功、该文件的完整记录已写入
- **THEN** 系统 SHALL 广播一条发布结束事件
- **AND** 该事件 SHALL 在完整记录写入**之后**发出——发布动作与完整记录写入之间 SHALL NOT
  插入任何其他挂起点（那个窗口内进程被杀会留下「暂存已消失、记录却不完整」的状态）

#### Scenario: 发布失败

- **WHEN** 发布因空间不足、权限被撤或描述符失效而失败
- **THEN** 系统 SHALL NOT 广播额外的发布事件；失败经既有的可恢复中断路径冒泡
- **AND** 前端 SHALL 在该会话进入非活跃状态时清除发布态，不留残影

#### Scenario: 零耗时的空文件补发布

- **WHEN** 对没有数据块可等的空文件执行补发布
- **THEN** 系统 SHALL NOT 广播发布事件（该路径零耗时，广播只会刷屏）

### Requirement: 超过三秒的本机阶段必须带百分比

任何预期超过约 3 秒的本机阶段 SHALL 展示百分比或剩余时间，SHALL NOT 只展示一个不确定态的
指示器。发布阶段在目标位于系统文档提供方托管的位置时是全量字节拷贝，属于该范围。

字节级进度的上报 SHALL 由实际执行拷贝的那一层承担，SHALL NOT 为此修改三端共用的宿主端口
签名——其余平台的发布是常数时间操作，没有可上报的循环。

#### Scenario: 外部存储目标的发布

- **WHEN** 发布目标由系统文档提供方托管，发布通过顺序拷贝完成
- **THEN** UI SHALL 展示「正在保存」及其百分比
- **AND** 百分比 SHALL 来自拷贝循环实际写出的字节数

#### Scenario: 同卷重命名的发布

- **WHEN** 发布通过原子重命名完成（常数时间）
- **THEN** UI SHALL 展示「正在保存」但不要求百分比——该状态通常一闪而过

#### Scenario: 拷贝开始前的准备也计入

- **WHEN** 发布在真正开始写字节之前还需要逐层建立目标目录
- **THEN** 「正在保存」状态 SHALL 从进入发布流程时开始展示，而不是从写第一个字节开始

### Requirement: 活跃传输展示剩余时间

处于活跃传输状态的会话，其展示面 SHALL 包含剩余时间。

主表面 SHALL 同时展示百分比、已传/总量、速度与剩余时间四项。次级表面（列表行、卡片、
系统通知）SHALL 至少展示百分比与剩余时间；空间只容得下速度与剩余时间之一时，SHALL 优先
保留剩余时间。

剩余时间算不出时 SHALL 展示占位，SHALL NOT 让该信息位整块消失。

#### Scenario: 非活跃状态

- **WHEN** 会话处于已完成、失败、暂停或等待对方接受状态
- **THEN** UI SHALL NOT 展示剩余时间（暂停态本就没有速度，展示剩余时间等于报告一个不存在
  的等待）

#### Scenario: 速度尚未成立

- **WHEN** 握手初期或传输停滞，速度不足以推算剩余时间
- **THEN** UI SHALL 在该信息位展示占位文案，SHALL NOT 移除该信息位

