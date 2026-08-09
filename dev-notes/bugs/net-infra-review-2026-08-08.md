# `2b5ac36a feat(net)` 审查发现

> **来源**：2026-08-08 跑 `/code-review high` 时，diff 范围连带覆盖了当时本地未推送的
> `2b5ac36a feat(net): 打通 relay 三态推送链路，引入 InfraLink 读模型`。这些发现**不属于**
> 同批推送的 mobile 改动（`5b42c937`），也**一行都没有改动过**。
>
> **核实状态**：下列每条的机制与 `file:line` 都经人工复核（不是原样转录审查输出）。
> 第 5 条经 `cargo test -p swarmdrop-core` 实测**当前不触发**，其余为静态复核。
>
> **当前状态：全部处置完毕（2026-08-09）。** 第 5 条在 `3a7466c3` 里已随手修掉；其余五条
> 本轮修复，另附一条对第 1 条**机制描述**的更正（结论对、触发路径错）。逐条处置见下方各节
> 末尾的「处置」段。

## 摘要

| # | 位置 | 类型 | 严重度 | 处置 |
|---|---|---|---|---|
| 1 | `crates/core/src/infra/supervisor.rs:88` | 行为正确性：开关失效 | **高**（触发路径比原文窄） | ✅ 改判据 + 2 条回归测试 |
| 2 | `crates/core/src/network/event_loop.rs:49` | 冗余推送 | 中 | ✅ 删边沿推送 |
| 3 | `src/components/network/node-status-sheet.tsx` 等 | 文案（桌面端，非 net） | 中 | ✅ 改 `<Plural>` + 补 en/zh-TW |
| 4 | `crates/core/src/runtime.rs:149` | 冷启动窗口 + 重复注册 | 中 | ✅ 闸门下沉到调用点 |
| 5 | `crates/core/src/infra/link.rs:141` | 潜在 panic（当前不触发） | 低 | ✅ 已在 `3a7466c3` 修 |
| 6 | `crates/core/src/network/manager.rs:318` | dead code + 回归风险 | 低 | ✅ 删除 |

---

## 1.（高）`public_reachability` 开关可被静默绕过

**现象**：用户在设置里关掉「公网可达性」后，本机仍可能继续对公网中继建立 relay
reservation。

**机制**：`exclusion_for`（收敛闸门的唯一定义处）用 `scope` 判断是否公网候选：

```rust
// crates/core/src/infra/supervisor.rs:88
if matches!(candidate.scope, CandidateScope::Public) && !self.public_reachability {
    return Some(InfraExclusion::PublicReachabilityDisabled);
}
```

但 `scope` 由 `CandidateScope::infer()` 按**合并后的全部地址**推断，语义是「任一私网/回环
地址即判 `Lan`」（`candidates.rs:34-40`），而候选的 `addrs` 表**只增不减**
（`candidates.rs:144-147`）。于是一台公网中继只要被 mDNS 顺带看到过一次私网地址，
`scope` 就**永久停在 `Lan` 再也回不去** —— `matches!(scope, Public)` 恒假，
闸门不再返回 `PublicReachabilityDisabled`，`wants_reservation` 保持为真。

**这个 latch 在同一个提交里已经被完整识别**，就写在 `manager.rs:377-381`：

> `scope` 是收敛环的**放行闸**……而 `upsert` 现在按累加后的全部地址重算它、地址表又只增
> 不减——于是一台公网中继只要被 mDNS 顺带看到过一次私网地址，`scope` 就永久停在 `Lan`
> 再也回不去，本机会在整个会话里谎报「不可公网可达」。**放行闸与能力声明是两件事，
> 不能共用一个位。**

那里据此把 `public_reachable` 改成了**地址判据**绕开 latch。但被自己点名的「放行闸」
本身没改，仍然读那个 latch 过的位。即：坑已认出、A 处已绕开、B 处留着。

**注**：latch 本身是刻意设计（`candidates.rs:278` 的测试注释写明「二次 upsert 不会把它
翻回去」），问题不在 latch，在于把它当成了 `public_reachability` 的放行闸。

**建议**：按 `manager.rs` 那条注释的思路，让 `exclusion_for` 也改用地址判据
（「该候选是否**持有**公网地址」），把「放行闸」与「能力声明」彻底拆开，不共用 `scope` 位。

---

**⚠️ 更正（2026-08-09 复核）：结论成立，但上面那句机制描述不可达。**

「一台公网中继只要被 mDNS 顺带看到过一次私网地址」——**mDNS 碰不到引导节点**。两条学习路径
按 agent 前缀互斥，且各自只留一种 scope 的地址：

| 路径 | 准入 | 地址过滤 |
|---|---|---|
| `learn_candidate`（`supervisor.rs:187`） | `is_bootstrap_agent` → `swarm-bootstrap/` | `usable_public_addrs`，只留公网 |
| `maybe_register_lan_helper`（`event_loop.rs:150`） | `is_swarmdrop_agent` → `swarmdrop/` + `lan-helper` cap | `usable_lan_candidate_addrs`，只留私网 |

两个前缀不重叠（`host/src/device.rs:274,277`），同一个 peer 进不了两条路。内置的
`47.115.172.218` 的 scope 稳定在 `Public`，闸门当时是有效的。

**真正可达的触发**：自建 bootstrap 跑在同一局域网、用户按**私网地址**把它加进候选表
（`HostConfigured`），随后 identify 把它的公网地址并进来 → `infer` 判 `Lan` → 一台真·公网
中继绕过闸门。窄，但真，且开关**静默**失效。

**处置（已修）**：没有给闸门另写一份地址判据，而是**把 `infer` 本身翻成能力声明**——
`CandidateScope::infer` 现在判「持有非 circuit 的公网可路由地址」即 `Public`
（谓词 `CandidateScope::is_infra_public_addr`，`usable_public_addrs` 共用它）。

这么选是因为 `scope` 有**两个**消费方：闸门（`exclusion_for`）与读模型下发给三端的
`scope`（shared-view / `node_health.rs` 拿它算 `configuredLanOnly`）。只改闸门会造出新的
不一致——混合地址候选被拦下，但 UI 的 `publicLinks` 过滤按 `scope === "public"` 走，
筛不到它，于是用户看到的是红色的「找不到任何节点」而不是中性的「是你关的开关」。
一个位、一个含义、两边都读。

回归测试：`candidates.rs` 的 `scope_stays_public_once_a_public_addr_is_known` /
`a_circuit_only_candidate_is_not_public`、`supervisor.rs` 的
`public_reachability_off_also_stops_a_mixed_addr_relay`（后者同时断言读模型能说出
`PublicReachabilityDisabled`，不只是「没建 reservation」）。

---

## 2.（中）relay 每次状态转换，完整 `NetworkStatus` 推送两次

**机制**：`run_event_loop` 里有两条独立的 `tokio::select!` 分支都会
`publish_network_status`：

- `crates/core/src/network/event_loop.rs:49-54` —— `RelayReservationAccepted / Lost`
  的 NetEvent 边沿分支；
- `crates/core/src/network/event_loop.rs:283-285` —— `relays_watcher.updated()` 分支。

而 net 侧一次 reservation 成功会**同时**改 relay state（触发 watch）并 emit 事件，两条轨
都会 fire。

`event_loop.rs:247-255` 解释了 watch 为什么必需：`RelayState::Connecting` 与
`Failed { last_error }` **没有任何 NetEvent 对应物**。但这反过来说明 `Accepted` / `Lost`
已被 watch 完全覆盖 —— 那两个 NetEvent 分支是冗余的。

同处注释写的「不会造成推送风暴：`set_relay_state` 用 `send_if_modified` 做了值相等去重」
说的是 **watch 内部**的去重，管不到「两条轨各推一次」这件事。

**代价**：每次转换多一次完整 `NetworkStatus` 重建（候选快照克隆 + `watch_relays()` /
`watch_conns()` 深拷贝），移动端是多一次完整 uniffi 跨越（含 `infra_links`）。因为退避是
2s→75s 而非每 tick，不构成风暴，属于可优化。

**建议**：删掉 `event_loop.rs:49-54` 那两个 NetEvent 分支的 publish（重建逻辑本就由
`InfraSupervisor` 负责，该分支注释自称「这里只推状态视图」），统一由 watch 一条轨推。

**处置（已修）**：照办。两个边沿分支保留（`handle_event` 仍要折叠 `ever_active`），只去掉
`publish_network_status`。复核确认 `actor.rs` 在 emit 前必然先写 watch——`Accepted` 走
`:1321` 的 `set_relay_state`，`Lost` 走 `:1118` 的 `set_relay_failed`（其 listener guard
在此路径上已被上一行的 `!any(...)` 保证放行）。

**顺带修掉一处原文没提的**：reservation 每轮续期都发 `Accepted { renewal: true }`，watch 侧
`send_if_modified` 判值相等**刻意静音**（`actor.rs:1320` 的注释就是这么说的），而这条 NetEvent
分支把静音拆了回来——每个 relay 每个续期周期都在推一份一模一样的全量状态。

两条轨到达顺序不定（`select!` 随机），已论证不构成问题：watch 只承载 relay 三态，
`ever_active` 由 `handle_event` 折叠，而 `deriveInfraLinkState` 先判 `relay == active` 才看
`everActive`；两者唯一同时生效的组合（曾活跃、现失败）里 `ever_active` 早已由更早那次
`Accepted` 置位。该论证连同「加新字段要重新过一遍」写进了代码注释与 `net-kernel.md`。

---

## 3.（中）英文界面显示 "1 transfer(s)" —— 桌面端，非 net

**现象**：停节点 / 重启节点的警告文案在英文下出现括号复数占位。已确认 catalog 内容：

```
src/locales/en/messages.po:1579
  "{activeTransferCount} transfer(s) in progress will be interrupted."
src/locales/en/messages.po:2348
  "Restarting will interrupt {activeTransferCount} transfer(s) in progress."
```

停节点警告**最常见的就是 1 个传输**，所以用户看到的正是那个占位形态。

**相关文件**：`src/components/network/stop-node-sheet.tsx`、
`src/routes/_app/settings/-settings-primitives.tsx`。

**建议**：改用 Lingui 6 的 `<Plural value={...} one="..." other="..." />`（本 catalog 里
别处已经在用）。

**处置（已修）**：两处都改成 `<Plural>`，en / zh-TW 译文补齐（`pnpm i18n:extract` 三份
catalog missing 归零）。⚠️ 原文写的 `stop-node-sheet.tsx` **已不存在**——它在
`3a7466c3` 里与 `start-node-sheet.tsx` 一起合并进了 `node-status-sheet.tsx`。
移动端**未照做**：那两句文案不在移动端，无需引入 `Intl.PluralRules` 风险。

> ⚠️ 移动端若照做，务必确认 `@formatjs` 的 `Intl.PluralRules` polyfill 仍在
> （见 `mobile/dev-notes/knowledge/toolchain.md` 的 Hermes Intl 条目）—— 那条是历史崩因。
> 桌面端走 WebView，无此约束。

---

## 4.（中）冷启动有无 relay 预约窗口，且每个 bootstrap 注册两次

**机制**：`crates/core/src/runtime.rs:149-163` 启动时只注册 kad 角色：

```rust
endpoint.add_infrastructure_peer(peer, InfraRoles { kad_server: true, relay: false }).await
```

relay 那一半改由 `InfraSupervisor::tick` 负责。于是每次冷启动，每个 bootstrap 节点先以
`relay: false` 注册一次，约一个 tick 后再以 `kad + relay` 注册一次。

**这是有意的权衡**，注释（`runtime.rs:140-147`）写明了原因：此前用
`InfraRoles::bootstrap()`（kad + relay），关掉「公网可达性」的用户照样在启动时建了公网
reservation，绕过了那道闸门；而整段又不能删（`public_reachability=false` 时公网候选一次
`add_infrastructure_peer` 都不发，kad 路由表拿不到公网种子，`dht.bootstrap()` 与在线记录
发布全塌）。

**代价**：首次可达性被推迟约一个 tick；注册工作做两遍。

**建议**：把闸门查询放进启动调用本身（启动时就读 `public_reachability` 决定 relay 角色），
而不是把整件事推迟到 tick。这样既保住闸门，又不留窗口。

**处置（已修，但不能照字面做）**：直接读全局 `public_reachability` 一刀切是错的——
`bootstrap_node_addrs()` 同时返回内置节点与用户自定义节点，后者可能是私网的，一刀切会把
LAN 候选也拦掉、tick 再加回来，窗口从「公网候选」挪到「LAN 候选」而已。

正确形态是**复用与 `exclusion_for` 同一条判据**：
`relay = public_reachability || CandidateScope::infer(&peer.addrs) == Lan`。
这在第 1 条修完之后才成立——判据统一了，这里才有一条可复用的、不会与收敛环分歧的谓词。
所以这两条是同一刀改的，顺序不能反。

---

## 5.（低）`debug_assert` 位于状态热路径 —— 当前不触发

**机制**：`exclusion_for` 内有

```rust
// crates/core/src/infra/supervisor.rs:90-93
debug_assert!(candidate.roles.relay_server, "候选表当前不产生纯 kad 候选；……");
```

`wants_reservation` 在到达它之前有 `candidate.roles.relay_server &&` 短路，但
`build_infra_links`（`link.rs:141`）对**每个**候选都调用它、无守卫。而
`build_network_status` 几乎每个网络事件都会调 —— 一旦出现纯 kad 候选，debug build 与
`cargo test` 会在状态热路径 panic。

**已实测：当前不触发。** `cargo test -p swarmdrop-core` 全绿（25 + 1 + 1 passed）；
产生 `relay_server: false` 的那处在 `candidates.rs:335` 的**测试代码**内，未走到
`build_infra_links`。assert 自身的注释也说明「纯 kad 候选今天不存在」。

**风险点**：同一提交把 `ensure_relay_intent` 改成了 `ensure_infra_intent(peer, roles)`
（caller 提供 roles），纯 kad 候选比以前更容易被构造出来。assert 消息自己写着
「出现即说明有新的写入方，需同步补 `InfraExclusion` 变体」。

**建议**：要么给 `build_infra_links` 加与 `wants_reservation` 同样的短路，要么按 assert
消息的指引补 `InfraExclusion` 变体（`link.rs` 里紧跟着的 `relay: None` 映射本就已正确
处理纯 kad 情形，只有 assert 会炸）。

**处置：已在 `3a7466c3` 修**（本清单写成时那个提交还没落）。取的是第一条路的变体——
`exclusion_for` 开头 `if !candidate.roles.relay_server { return None; }`，短路放在闸门自己
而不是调用方，`build_infra_links` 不必知道这条规则。**没有**补 `InfraExclusion` 变体：
「不承担该角色」与「承担但被拦下」是两回事，前者已由 `relay: None` + `roles.relay_server
== false` 表达（shared-view 判 `seedOnly`），多一个判别码只是让三端多一个渲染不出差异的
分支，还要过三条 codegen。回归测试 `kad_only_candidate_is_neither_converged_nor_excluded`。

---

## 6.（低）`active_relay_peers()` 是 dead code，且正是新注释禁止的用法

**现状**：定义在 `crates/core/src/network/manager.rs:318`，全仓唯一「引用」是
`crates/core/tests/infra_reconcile.rs:151` 的一行**注释**。

**为什么不能就这么留着**：同一提交给 `build_network_status` 加的注释明确规定 ——
`relay_peers` / `relay_ready` **必须**从同一份 `infra_links` 派生，**不能再读一次
`watch_relays`**（两次读会让 `infra_reconcile` 那条测试变成偶发红）。而
`active_relay_peers()` 做的恰恰就是那次被禁止的第二次读，还是 `pub`。留着它，下一个调用方
就会把「`relay_ready: true` 但 `infra_links[p].relay: Failed`」这个矛盾带回来。

**建议**：删除；或降为私有并由 `build_infra_links` 内部使用。

**处置（已修）**：删除。没有降为私有——`build_infra_links` 已经一次性读了
`watch_relays().get()` 快照，再包一层「只取 Active 的 key」的辅助函数没有调用方。
`infra_reconcile.rs:151` 那条注释同步改写成用判据表述（「有人绕开 `build_infra_links` 又读了
一次 `watch_relays`」），不再指名一个已不存在的函数。

---

## 附：同刀一并做掉的（不在原清单内）

**删除 `DiscoveryMode`（「发现模式：自动 / 仅局域网」）全端。** 它零行为效果
（`discovery_mode()` getter 全仓无调用方，字段只被写入和回显），却出现在桌面下拉与移动端
双卡里，且改它会触发「需重启节点」——用户中断在途传输重启一遍，什么也没发生。
它与 `public_reachability` 的语义重叠是删的依据：后者已经表达了「别让跨网设备找到我」，
而「发现模式」自己的描述文案还写着「仅局域网模式下仍可被跨网访问，除非关闭公网可达性」。

删除面：core 枚举 + `NetworkRuntimeConfig` / `NetworkStatus` 两处字段 + 三条 codegen
（specta / uniffi / wasm）+ 桌面设置行 + 移动 `DiscoveryModeOption` 双卡与节点弹窗里的
「发现方式」行 + 两份 preferences store（含移动端的持久化校验分支）+ 四份 catalog
+ `crates/web/src/node.rs` 硬编码的 `LanOnly` + openspec spec 里那条 SHALL。
移动端分组标题 `发现方式` → `可达性`（该分组现在只剩公网可达性与局域网协助两个开关）。

决策记录在 `dev-notes/research/2026-08-tri-platform-network-status.md` 的开放问题 2。

## 附：不在本清单内的

审查同时覆盖了 `5b42c937 fix(mobile)`（视频闪退修复 + ErrorBoundary + 导航条重构）的
全部发现，那些**已在该提交内修完**，不在此列。
