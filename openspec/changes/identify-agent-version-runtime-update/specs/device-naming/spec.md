# device-naming

## REMOVED Requirements

### Requirement: 改名对已连接对端的生效时机是显式的过渡限制

**Reason**: 该条款是 `device-config-port`（C5）自己标注的过渡限制 —— 它写明「消除它是
`identify-agent-version-runtime-update`（C6）的职责」。本 change 正是那次消除：identify 的
`agent_version` 已可运行时逐连接更新，改名对已连接对端在一个 RTT 内生效，三端都不再重启节点或
刷新页面。该条款的每一句在本 change 之后都是错的：

- 「本能力 SHALL 只保证改名后**新建立**的对外表示携带新名字」——现在既有连接同样携带；
- 「各端 SHALL 明确告知用户生效条件：桌面与移动在改名后重启节点，Web 提示刷新页面」——
  这些提示已从三端删除，重启编排本身也已删除；
- 「保存动作与节点重启 SHALL 被表达为两件独立的事」——已不存在「节点重启」这个动作。

留着它，`device-naming` 与 `live-device-rename` 两份能力会给出互相矛盾的断言：一份要求对端
看到旧名字直到重启，另一份要求秒级看到新名字。

**Migration**: 由 `live-device-rename` 能力整体接管，无迁移动作。对应的新断言见
`live-device-rename` 的「改名对已连接的对端即时生效」（含离线对端与推送丢失的兜底场景）
与「改名不影响进行中的会话」。

**归档顺序**：本条删除以 C5 已归档为前提 —— C5 先归档把该 requirement 写进
`openspec/specs/device-naming/spec.md`，本 change 再归档才有东西可删。若本 change 先归档，
该删除会落空，`device-naming` 里仍会留下过渡条款。
