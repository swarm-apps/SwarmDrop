# transfer-offer

## ADDED Requirements

### Requirement: 接受入站 Offer 的失败必须落在「越线」的正确一侧

接受入站 Offer 的过程 SHALL 以「向对端回复 `OfferResult { accepted: true }`」为**越线点**
（point of no return）—— 越过它之后对端即开始推送数据，本机无法单方面撤回。

**能挪的可失败步骤 SHALL 全部挪到越线之前。** 只写本机状态的步骤（保存位置写入、
`offered → active` 的状态转换）SHALL NOT 排在应答之后。

**越线之前**的每一步失败 SHALL 保持该 offer 可重试：挂起 offer SHALL 仍在待决表中、
对端的应答通道 SHALL 未被 drop，用户再点一次「接受」能重新走完整流程。

**越线之后**的失败 SHALL NOT 返回错误。这类失败只影响本机记账，而对端已按成功推进；
返回错误会让用户对一件已经发生的事采取纠正动作。它们 SHALL 记入警告日志。

#### Scenario: 保存位置写入失败（越线前）

- **WHEN** 用户点击接受，写入保存位置的数据库操作失败
- **THEN** 操作返回错误，该 offer 仍在待决表中并保持在 UI 上，对端的应答通道未关闭，
  用户可以再次点击接受

#### Scenario: 状态转换写入失败（越线前）

- **WHEN** 用户点击接受，`offered → active` 的状态转换写库失败
- **THEN** 操作返回错误，offer 可重试；SHALL NOT 已经向对端回复接受

#### Scenario: 应答通道已关闭（越线未发生）

- **WHEN** 回复对端时发现应答通道已被 drop（对端 RPC 已超时）
- **THEN** 对端并未收到接受，本机 SHALL 回滚：移除已注册的接收 actor、把会话转为终态，
  并返回错误

#### Scenario: 接收 actor 的注册时机

- **WHEN** 向对端回复接受
- **THEN** 接收 actor SHALL 已经注册完毕 —— 对端收到接受后立即打开数据面流，
  actor 未就绪会导致 Hello 被拒

### Requirement: 拒绝入站 Offer 遵循同一条越线规则

拒绝入站 Offer 的状态转换（`offered → rejected`）SHALL 写在向对端回复之前。

转换失败时 SHALL 返回错误并把 offer 放回待决表；SHALL NOT 在已回复对端之后才报「拒绝失败」
—— 那时对端已按拒绝收尾，用户再点一次只会得到「offer 不存在」。

#### Scenario: 状态转换失败

- **WHEN** 用户点击拒绝，`offered → rejected` 的写库失败
- **THEN** 操作返回错误，offer 仍在待决表中，对端未收到任何应答，用户可以再次点击拒绝

#### Scenario: 拒绝已写成但应答通道已关闭

- **WHEN** 本机已记账为 rejected，回复对端时发现通道已关闭
- **THEN** 操作成功返回 —— 对端的 RPC 早已超时，双方结论一致，SHALL NOT 回滚也 SHALL NOT 报错
