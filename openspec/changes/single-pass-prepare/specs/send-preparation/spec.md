## ADDED Requirements

### Requirement: 发送前置准备对每个文件只读一遍源数据

`TransferManager::prepare` SHALL 对每个待发送文件只完整读取一遍源数据，在同一遍中同时产出该文件的 `checksum` 与逐块验签树（outboard）。系统 MUST NOT 为求 `checksum` 单独跑一遍哈希——验签树构建返回的 root 就是 `checksum`。

#### Scenario: 单文件准备只触发一轮源读取

- **WHEN** 对一个 `size` 字节的文件调用 `prepare`
- **THEN** 该文件上 `FileAccess::read_source_chunk` 的累计请求字节数 MUST 恰好等于 `size`，且每个字节区间 MUST 只被请求一次

#### Scenario: checksum 与验签树同源

- **WHEN** `prepare` 为某文件产出 `PreparedFile`
- **THEN** `PreparedFile.checksum` MUST 等于同一次构建返回的验签树 root 的十六进制表示，二者 MUST NOT 来自两次独立的读取

#### Scenario: 准备期间源文件被外部修改

- **WHEN** 源文件在 `prepare` 读取过程中被外部改短
- **THEN** 系统 MUST 以可归因于本地源文件的错误终止该次准备，而 MUST NOT 产出 `checksum` 与验签树互不匹配的 `PreparedFile`

### Requirement: 准备进度覆盖该阶段的全部真实工作量

准备阶段发出的进度事件 SHALL 覆盖 `prepare` 在源数据上做的全部工作，进度分母 MUST 等于本批文件的总字节数，且 MUST NOT 存在任何「有工作在做但不发事件」的静默区间。

#### Scenario: 进度单调推进至满

- **WHEN** 一次多文件 `prepare` 从开始跑到结束
- **THEN** 收到的进度事件的 `bytes_hashed` MUST 单调不减，且最后一条 MUST 等于 `total_bytes`

#### Scenario: 单个大文件不出现进度停滞

- **WHEN** `prepare` 正在处理某个远大于单次读取粒度的文件
- **THEN** 进度事件 MUST 在该文件的读取过程中按节流间隔持续发出，而 MUST NOT 在该文件内部出现「读取仍在进行但进度不再变化」的区间

#### Scenario: 批次结束发出终局事件

- **WHEN** 一批文件的准备全部完成
- **THEN** 系统 MUST 发出一条不受节流限制的终局进度事件，其 `completed_files == total_files` 且 `bytes_hashed == total_bytes`

### Requirement: 准备进度按 preparedId 广播并跨页面存活

准备进度 SHALL 以广播事件形式投递给宿主，由 `prepared_id` 区分并发的准备批次。宿主 SHALL 把进度落进按 `prepared_id` 索引的共享状态，使其在发起该次准备的界面被卸载后依然可读。系统 MUST NOT 把准备进度的投递绑定在单次调用的生命周期上。

#### Scenario: 非 UI 发起的准备同样产生进度

- **WHEN** 准备由 UI 之外的入口发起（如本机 MCP 工具、收件箱转发）
- **THEN** 进度事件 MUST 与 UI 发起时一样被投递给宿主，而 MUST NOT 因为「没有对应的调用通道」被丢弃

#### Scenario: 界面离开后重新进入

- **WHEN** 用户在准备进行中离开发起页面，随后返回
- **THEN** 界面 MUST 能从共享状态中读回当前批次的进度并继续呈现

#### Scenario: 并发准备互不覆盖

- **WHEN** 两个准备批次同时进行
- **THEN** 两者的进度 MUST 各自按自己的 `prepared_id` 存放，MUST NOT 相互覆盖

#### Scenario: 批次结束后清理

- **WHEN** 一个准备批次结束（成功或失败）
- **THEN** 宿主 MUST 清除该 `prepared_id` 的进度条目与活跃标记，使下一次准备 MUST NOT 在首条事件到达前呈现上一批次的残留

### Requirement: 准备阶段不存在会话标识

准备阶段 SHALL 只由 `prepared_id` 标识。系统 MUST NOT 要求准备进度携带 `session_id`，也 MUST NOT 把准备进度挂进任何按 `session_id` 索引的状态——会话记录在准备完成之后、发出 Offer 时才创建。

#### Scenario: 进度事件不含会话标识

- **WHEN** 定义准备进度事件类型
- **THEN** 它 MUST NOT 包含 `session_id` 字段

#### Scenario: 准备进度不进入会话进度模型

- **WHEN** 宿主处理准备进度事件
- **THEN** 它 MUST NOT 写入按 `session_id` 索引的进度或投影状态，也 MUST NOT 因此引入 active 之外的会话状态

### Requirement: 源读取的返回长度必须精确匹配请求

准备阶段用于构建验签树的读取器 SHALL 校验 `read_source_chunk` 的返回长度**严格等于**请求长度，不足与超长都 MUST 判为宿主违约并终止该次准备。验签树构建 MUST NOT 请求越过文件末尾的字节，因此等长是可断言的。

#### Scenario: 宿主返回超长数据

- **WHEN** 宿主对 `read_source_chunk(offset, len)` 返回多于 `len` 的字节
- **THEN** 系统 MUST 以「违反契约」为由响错，而 MUST NOT 截断后继续，也 MUST NOT 把非法长度送进哈希器

#### Scenario: 宿主返回不足数据

- **WHEN** 宿主对文件末尾之内的 `read_source_chunk(offset, len)` 返回少于 `len` 的字节
- **THEN** 系统 MUST 以宿主违约响错，错误信息 MUST 能定位到具体文件

#### Scenario: 读取请求不越界

- **WHEN** 验签树构建向读取器发出请求
- **THEN** 每个请求 MUST 满足 `offset + len <= size`
