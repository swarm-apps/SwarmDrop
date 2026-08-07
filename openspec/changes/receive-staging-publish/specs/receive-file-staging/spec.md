# receive-file-staging

## ADDED Requirements

### Requirement: 接收写入分为 staging 与 publish 两个阶段

接收方写入文件 SHALL 分成两个阶段：**staging**（把收到的数据块随机写入一个暂存位置）
与 **publish**（把收齐的文件发布到用户选定的目标位置，并返回它最终所在的 URI 及其父目录）。

宿主 SHALL 自行决定 staging 位置，端口 SHALL NOT 规定它在哪——当 staging 与目标位于同一
存储卷时，宿主 SHALL 采用原子重命名而非拷贝。

#### Scenario: 目标与 staging 同卷

- **WHEN** 一个文件收齐，且目标位置与 staging 位于同一存储卷
- **THEN** publish SHALL 通过原子重命名完成，SHALL NOT 产生额外的全量拷贝

#### Scenario: 目标位于外部存储

- **WHEN** 一个文件收齐，且目标位置由系统文档提供方托管（无法重命名过去）
- **THEN** publish SHALL 顺序拷贝到目标位置，完成后删除 staging

### Requirement: staging 位置必须由本应用完全拥有

staging SHALL 位于文件描述符生命周期完全由本应用控制的存储位置。

接收过程中的随机写（含定位偏移）SHALL NOT 施加于由外部文档提供方授予的文件描述符——
这类描述符可能在本应用无从感知的情况下失效，使已打开的文件通道在自身仍报告为"打开"
的状态下于下一次定位操作失败。

外部目标位置 SHALL 只在 publish 阶段被触碰，且只承受顺序写。

#### Scenario: 用户把接收目录设为系统公共目录

- **WHEN** 用户选择由系统文档提供方托管的目录作为接收位置
- **THEN** 数据块 SHALL 写入应用私有的 staging，SHALL NOT 直接随机写入该目录中的文件

#### Scenario: 外部描述符在传输中途失效

- **WHEN** 某个外部授予的文件描述符在使用期间失效
- **THEN** 系统 SHALL 报出可识别的"句柄已关闭"错误，SHALL NOT 让底层错误码裸露到用户界面

### Requirement: staging 位置可由文件元信息确定性重建

staging 位置 SHALL 是文件元信息（目标目录与相对路径）的确定性函数，
SHALL NOT 依赖传输会话标识或任何仅存在于内存中的状态。

同一目标目录下的同一相对路径 SHALL 映射到同一个 staging 位置；
不同的目标目录或不同的相对路径 SHALL NOT 映射到同一个 staging 位置。

#### Scenario: 进程重启后续传

- **WHEN** 应用被杀死后重启，用户恢复一个未完成的接收
- **THEN** 系统 SHALL 仅凭文件元信息定位到原有的 staging 并在其后继续写入，
  SHALL NOT 从头重传已落盘的部分

#### Scenario: 两个目录下的同名文件

- **WHEN** 两个不同的目标目录下存在相对路径相同的文件
- **THEN** 它们 SHALL 使用互不相同的 staging 位置

### Requirement: 单个文件收齐即发布

publish SHALL 在**单个文件的分块位图收齐时**发生，SHALL NOT 推迟到整个传输会话结束。

已发布的文件 SHALL 不再持有任何打开的写入句柄。

会话终态处理 SHALL 只负责会话级语义（终态转换、收件箱索引、完成事件），
SHALL NOT 承担任何文件级的最终化工作。

#### Scenario: 多文件传输中途中断

- **WHEN** 一个包含多个文件的接收在传输到一半时中断
- **THEN** 已收齐的文件 SHALL 已经位于用户选定的目标位置，
  SHALL NOT 停留在 staging 等待整个会话完成

#### Scenario: 大批量文件的暂存占用

- **WHEN** 接收一批文件
- **THEN** staging 的磁盘占用峰值 SHALL 不超过单个最大文件的大小，
  同时打开的写入句柄数 SHALL NOT 随文件数量增长

#### Scenario: 空文件

- **WHEN** 传输包含一个大小为 0 的文件
- **THEN** 该文件 SHALL 同样经由 publish 落到目标位置

#### Scenario: 续传中未参与本次传输的已完成文件

- **WHEN** 恢复传输时某个文件的分块位图已经完整
- **THEN** 系统 SHALL 视其为已发布，SHALL NOT 再次为它创建 staging 或重新发布

### Requirement: 已发布的文件不再接受数据块

一个文件发布之后，系统 SHALL 拒绝属于它的任何后续数据块，
SHALL NOT 为它重新创建暂存，SHALL NOT 覆盖已落地的文件。

续传开始时，分块位图已完整的文件 SHALL 被视为已发布。

#### Scenario: 已发布文件的数据块再次到达

- **WHEN** 某文件已发布后，又收到一个属于它的数据块
- **THEN** 系统 SHALL 报错并中断本次数据通道，
  SHALL NOT 让用户目标位置上那个完整文件被覆盖

### Requirement: publish 只发布不校验

publish SHALL NOT 重新读取文件内容进行完整性校验——完整性由逐块验签在数据落盘前保证。

#### Scenario: 发布一个大文件

- **WHEN** 一个已收齐的大文件被发布
- **THEN** 系统 SHALL NOT 为校验目的再完整读取该文件一遍

### Requirement: publish 失败表示可重试的落地失败

publish 失败 SHALL 被视为"数据完好但未能落地"，而非数据损坏。

系统 SHALL 保留 staging，SHALL NOT 重置该文件的分块进度，
SHALL 将传输转入可恢复的中断状态而非不可恢复的失败状态。

#### Scenario: 目标位置空间不足

- **WHEN** publish 因目标存储空间不足而失败
- **THEN** staging SHALL 被保留，该文件的分块进度 SHALL NOT 被重置，
  用户 SHALL 能在腾出空间后恢复传输并只重做 publish

#### Scenario: 恢复后重新发布

- **WHEN** 用户在一次 publish 失败后恢复传输
- **THEN** 系统 SHALL 直接重试 publish，SHALL NOT 要求对端重传该文件的任何数据块

### Requirement: publish 可重入且失败时清理半成品

publish SHALL 可重复执行：目标位置已存在同名文件时 SHALL 覆盖它，
SHALL NOT 生成带序号后缀的副本。

publish 失败时 SHALL 尽力删除目标位置上的不完整产物。

#### Scenario: 拷贝中途失败

- **WHEN** publish 在向外部目标拷贝的过程中失败
- **THEN** 系统 SHALL 尽力删除目标位置上长度不足的产物，并保留 staging 供重试

#### Scenario: 进程在拷贝中途被杀死后恢复

- **WHEN** 应用在 publish 拷贝过程中被系统杀死，用户随后恢复传输
- **THEN** 系统 SHALL 依据 staging 仍然存在判定该文件尚未发布完成，
  并重新执行 publish 覆盖此前的不完整产物

### Requirement: 文件名在发布时不被系统改写

publish 到由系统文档提供方托管的目标时，SHALL 显式声明通用二进制内容类型，
以确保目标文件名与传输清单中的文件名逐字一致。

#### Scenario: 发布一个提供方会据类型改写扩展名的文件

- **WHEN** 向系统文档提供方发布一个扩展名可能与推断类型不匹配的文件
- **THEN** 落盘的文件名 SHALL 与传输清单中的文件名完全一致，SHALL NOT 被追加或替换扩展名

### Requirement: 取消传输后已发布的文件保留

用户取消一个多文件接收时，已经 publish 的文件 SHALL 保留在目标位置。

未发布的 staging SHALL 被删除。

#### Scenario: 取消一个部分完成的多文件接收

- **WHEN** 用户取消一个已有部分文件收齐并发布的接收
- **THEN** 已发布的文件 SHALL 保留，尚未收齐的 staging SHALL 被删除
