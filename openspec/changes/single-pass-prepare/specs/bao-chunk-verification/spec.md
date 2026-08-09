## ADDED Requirements

### Requirement: 验签树根等于整文件 BLAKE3

逐块验签树的 root SHALL 恒等于源文件的标准 BLAKE3 整文件哈希，与所选 chunk group 尺寸无关。`FileInfo.checksum` SHALL 就是这个值的十六进制表示，wire 上 MUST NOT 为验签额外增加字段。

#### Scenario: 流式构建与扁平哈希一致

- **WHEN** 对任意大小的文件流式构建验签树
- **THEN** 返回的 root MUST 等于 `blake3::hash(整个文件内容)`

#### Scenario: 内存构建与流式构建同序

- **WHEN** 对同一份数据分别用内存构建与流式构建
- **THEN** 两者的 root MUST 相等，产出的 outboard 字节 MUST 逐字节相等

#### Scenario: 空文件

- **WHEN** 文件大小为 0
- **THEN** 构建 MUST 成功，root MUST 等于空输入的 BLAKE3 哈希

#### Scenario: 接收端由 checksum 还原验证根

- **WHEN** 接收端拿到 `FileInfo.checksum`
- **THEN** 它 MUST 能把该十六进制串解析回验签根并用于逐块验证，解析失败 MUST 判为协议错误

### Requirement: chunk group 尺寸等于传输块尺寸

验签树的 chunk group SHALL 与数据面的传输块尺寸 `CHUNK_SIZE` 相等，使每个传输块恰好对应验签树的一个叶子。系统 MUST NOT 采用比传输块更细的 chunk group——更细的粒度不产生任何可用的验证能力，只放大 outboard 体积、wire 上的 proof 开销与构建时的源读取次数。

#### Scenario: 常量对齐

- **WHEN** 定义验签树的 chunk group 常量
- **THEN** 它换算出的字节数 MUST 等于 `CHUNK_SIZE`

#### Scenario: 构建读取粒度与传输一致

- **WHEN** 流式构建验签树
- **THEN** 每次向源发出的读取请求长度 MUST 不超过 `CHUNK_SIZE`，使构建阶段的源读取次数与实际传输阶段处于同一量级

### Requirement: outboard 有效性由长度判定

持久化的 outboard SHALL 以「其字节长度是否等于当前 chunk group 下该文件大小对应的确定性长度」作为有效性判据。系统 MUST NOT 用「是否为空」作为唯一失效判据——那放不掉一个非空但格式已作废的 outboard，会使续传每次都以验签失败告终且永不触发重算。

#### Scenario: 长度不符判为失效并重算

- **WHEN** 从持久化载入的 outboard 长度不等于该文件大小对应的期望长度
- **THEN** 系统 MUST 视其为缺失、重新构建并回存，而 MUST NOT 把它喂进 proof 生成

#### Scenario: 单叶子文件的 outboard 合法为空

- **WHEN** 文件大小不超过一个 chunk group
- **THEN** 其期望 outboard 长度 MUST 为 0，空 outboard MUST 被判为有效而不触发重算

#### Scenario: 判据不泄漏底层树类型

- **WHEN** 恢复流程需要判定 outboard 有效性
- **THEN** 它 MUST 经本仓的验签模块提供的函数判定，MUST NOT 在恢复流程中直接构造底层 bao 树类型

### Requirement: proof 生成要求块落在 chunk group 边界

生成 proof 时，被证明的块 SHALL 起始于 chunk group 边界，且其结束位置 SHALL 要么落在 chunk group 边界上、要么等于文件末尾。系统 MUST 在生成前显式校验该前提并给出可读错误，MUST NOT 让违反前提的输入退化成难以与真实 IO 故障区分的底层读取错误。

#### Scenario: 对齐的整块

- **WHEN** 块的起止都落在 chunk group 边界上
- **THEN** proof 生成 MUST 成功

#### Scenario: 跨多个整叶子的块

- **WHEN** 块起始于 chunk group 边界、长度跨越多个叶子且结束于文件末尾
- **THEN** proof 生成 MUST 成功——校验 MUST NOT 把合法输入限制为「恰好一个叶子」

#### Scenario: 非对齐起点

- **WHEN** 块的起始偏移不是 chunk group 的整数倍
- **THEN** 系统 MUST 以明确的对齐错误拒绝，而 MUST NOT 返回底层读取错误

#### Scenario: 空块

- **WHEN** 块长度为 0（仅出现于 0 字节文件）
- **THEN** proof 生成 MUST 返回空 proof 而非报错，接收端 MUST 对称地把空 proof 解为空数据

### Requirement: 验签树构建的顺序读取前提由护栏测试钉死

验签树构建对源读取器发出的请求 SHALL 满足：偏移从 0 严格单调递增、每次长度不超过一个 chunk group、末次为精确剩余、累计长度恰好等于文件大小。该性质来自底层库的**实现事实而非其公开契约**（其读取器接口声明为随机读），而本仓的进度单调性与读取等长判据均依赖它，因此 SHALL 由专门的护栏测试断言。

#### Scenario: 记录调用序列的读取器

- **WHEN** 用一个记录全部 `read_at(offset, len)` 调用的读取器构建验签树
- **THEN** 记录的偏移序列 MUST 严格单调递增、每次 `len` MUST 不超过一个 chunk group、`len` 之和 MUST 等于文件大小

#### Scenario: 升级底层库后的警报

- **WHEN** 底层验签树库升级导致读取策略改变
- **THEN** 该护栏测试 MUST 失败，其测试名与注释 MUST 说明「进度单调性与读取等长判据同时失效」
