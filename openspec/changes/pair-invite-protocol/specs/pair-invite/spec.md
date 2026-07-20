# pair-invite

## ADDED Requirements

### Requirement: 邀请串编码可往返且大小写不敏感

`PairInvite` SHALL 编码为 `sdinvite` 前缀 + base32-nopad（小写规范形态）文本，
解码 SHALL 对 payload 大小写不敏感（支持二维码大写 alphanumeric 形态），且
编码→解码 roundtrip 后领域值逐字段相等。wire 采用 postcard 单变体 enum
（`InviteWire::V1`），未知变体判别码 SHALL 解码失败而非静默误读。

#### Scenario: roundtrip 与大写解码

- **WHEN** 生成邀请串后分别以原样和整串大写形式解码
- **THEN** 两者均成功且与原始 `PairInvite` 逐字段相等

### Requirement: 签名保护字段完整性

邀请串 SHALL 由发起方 NodeId 对应私钥签名（签名尾置，signable 覆盖含版本判别码
在内的全部前置字节），验签公钥从 `inviter_id` 恢复。任一字段（含
`transport_policy`、`display_hint`、地址提示）被篡改后 SHALL 解码失败
（`ParseError::Verify`），不进入配对流程。

#### Scenario: 篡改 transport_policy 被拒

- **WHEN** 将有效邀请串解码字节中的 `transport_policy` 从 LocalOnly 改为 Auto 并重编码（不重签）
- **THEN** 接收方解码返回验签失败，不发起任何连接

### Requirement: TTL 与一次性消费

发起端 SHALL 只持久化 `sha256(capability)` 与过期时间（明文 capability 不落盘/日志）。
收到 PairHello 时 SHALL 校验：invite 未过期、capability 哈希匹配、状态为 Pending 并以
原子 CAS 转入 Consumed——过期、哈希不匹配、已消费、已撤销均 SHALL 拒绝且不写信任记录。

#### Scenario: 并发双花仅一胜

- **WHEN** 两台设备用同一邀请串同时发起 PairHello
- **THEN** 恰有一台进入确认流程，另一台收到已消费拒绝

### Requirement: 配对成立需双向确认与身份一致

受邀方 SHALL 在连接建立后校验实际远端身份等于 `inviter_id`（不一致即中止）；
双方 SHALL 各自显式确认（展示设备名/平台/短指纹）后才写入长期配对记录；
任一步失败 SHALL 不产生信任记录。既有 Code/Direct 配对方式行为不变（双轨并存）。

#### Scenario: 完整 Invite 配对

- **WHEN** 设备 A 生成邀请、设备 B 解码后连接并完成 capability 校验与双向确认
- **THEN** 双方配对记录写入各自持久化层，后续可凭 NodeId 重连；该邀请串再次使用被拒
