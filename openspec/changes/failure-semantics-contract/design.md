# failure-semantics-contract 设计

## D1 —— 「越线」（point of no return）是一条可判定的规则，不是感觉

一个操作里存在一个时刻，过了它**对端的状态就已经改变、且本机无法单方面撤回**。本仓目前有
三处这样的时刻，形态完全一样：

| 操作 | 越线动作 | 越线后对端认为 |
|---|---|---|
| 接受配对 | `responder.send(PairingResponse::Success)` | 已配对 |
| 接受传输 | `responder.send(OfferResult { accepted: true })` | 可以推数据了 |
| 拒绝传输 | `responder.send(OfferResult { accepted: false })` | 本次传输结束 |

规则三条，**按优先级**：

1. **能挪的可失败步骤，全部挪到越线之前。** 这是最强的满足方式 —— 越线之后没有可失败的
   步骤，就没有「已经发生却报失败」的可能。`dispatch(Accept)` / `dispatch(Reject)` 只写
   本机 DB，本来就可以先做。
2. **越线之前**的失败必须**可重试** —— 「用户再点一次那个按钮」能重新走一遍。所以消费掉
   一次性资源（`pending.remove`）之后若还有可失败的步骤，失败路径必须把资源**放回去**。
3. **确实挪不动的**（收尾动作本身就依赖已经越线这件事），失败时不得返回 `Err`，
   记 `warn!` 即可。这时候的失败只影响本机记账，而对端已经按成功在走；报失败会让用户对一件
   **已经发生**的事采取纠正动作，两边越纠越歪。

第 3 条在本仓已有先例：`handle_cancel_impl` / `handle_pause_impl` /
`handle_peer_disconnected_impl` 的 dispatch 全是 `if let Err(e) = … { warn!(…) }` —— 它们是
对端信号的处理器，「已经发生」是入参而不是自己造成的，没有「挪到之前」这个选项。
`accept_and_start_receive` 有，所以它走第 1 条。

### `responder.send` 失败怎么办

那说明对端**没有**收到接受 —— 越线**没有发生**。此时本机会话已经是 `active`、actor 已注册，
必须回滚：撤掉 actor、`dispatch(Cancel)`、返回 `Err`。这是唯一需要主动补偿的分支。

拒绝路径不需要对称的补偿：拒绝本就是终态，本机记账已经完成，对端没收到只意味着它的 RPC
早就超时了 —— 双方结论一致。

## D2 —— 「一半成功」用返回值表达，不用 `Err`（本期**没有用上**）

`PairedDeviceCommit { device, persisted }` 是这个体例（上一个 change）。传输接受起初也照它
设计了 `AcceptOutcome { recorded: bool }`，三端接完之后回头看：**按 D1 第 1 条把
`dispatch(Accept)` 挪到越线之前后，越线之后一个可失败的步骤都不剩了** —— 那个 bool 恒为
`true`，三端各自维护一条永远不会触发的降级提示。

于是整条拆掉。留下的教训是 D1 的三条规则**有优先级**：先问「能不能挪」，挪不动才谈怎么
表达降级。反过来先设计降级返回值，会做出一个用不上的 API 还铺到三端去。

（同一天里 D3 因为一模一样的理由砍掉了 `IntegrityFailed`。判据长得不一样，病因是同一个：
**先设计了机制，没验证它到不到得了用户。**）

## D3 —— `AppError` 的 kind 拆到哪一层为止

判据只有一条：**这个 kind 能让 UI 给出与其他 kind 不同的、用户真能照做的建议吗？**

`Transfer` 的 104 处按这条筛，第一轮过了三类：

| 候选 kind | 覆盖 | UI 能说的话 |
|---|---|---|
| `SessionNotFound` | 会话 / 挂起 offer / 收件箱条目 / PreparedTransfer 不存在 | 「这条记录已经不在了，请回列表重新开始」 |
| `StorageFailed` | 写盘、OPFS、sink 写入失败 | 「保存失败，请检查磁盘空间或换个保存位置」 |
| ~~`IntegrityFailed`~~ | bao 逐块验签、checksum 不匹配 | 「文件校验未通过，请重新传输」 |

**第三个砍掉了**，因为判据还有第二问：**这个 kind 到得了 UI 吗？** 内容校验失败只发生在
`ReceiverActor` 里，而那条路径的失败走的是 `ActorReport::FatalError(String)` → 落库
`error_message` → 详情页渲染那个 String，**全程不经过 `kind`**。造出来就是一个永远不会被
任何文案表命中的判别码 —— 正是本节要防的那种churn，只是伪装成了「合理的分类」。

（移动端已经在用 `friendlyTransferError` 对那个 String 做正则匹配来出文案，桌面与 Web 没有。
那条通道要不要收成判别码是另一件事，本 change 明确不做。）

剩下的 ~70 处（锁中毒、JS 句柄类型错误、range 溢出、序列化失败、协议帧不合法）**全部留在
`Transfer`**。它们对用户是同一件事：「出了个你处理不了的问题」。为它们各造一个 kind 只会让
三端文案表膨胀，而每条文案都只能写成「出错了，请重试」的同义句。

### 迁移只碰「命令能收到」的调用点

同一条判据的第二问也决定了迁移范围：`SessionNotFound` 迁的是**用户点了按钮后能直接拿到**的
那 12 处（接受/拒绝/暂停/取消/续传/收件箱查询）。深在 actor 内部的「文件不存在: file_id=3」
「checkpoint bitmap 不存在」留在 `Transfer` —— 它们是内部不变量被破坏，不是「你指的那条不在了」，
而且同样走 String 通道。

**`Transfer` 因此不是「垃圾桶」而是「其余」** —— 区别在于它有判据、且判据写在 doc 里。
`Identity` 当年的问题不是承载得多，是**没有判据**，于是 peer_id 解析失败也往里塞。

### 协议不兼容为什么不单独出 kind

想过 `ProtocolMismatch`。放弃的理由：本仓 wire 版本协商在**连接建立阶段**就完成，走的是
`crates/net` 的协议路由、不经 `AppError`；`AppError::Transfer("transfer-data 协议错误: …")`
这类是**同版本内的帧格式异常**，对用户而言就是「对面客户端有毛病」，与「升级客户端」不是
同一个建议。证据不足，不造。

## D4 —— 三端文案表是三份，不是一份

移动端补 `errors.ts` 时的第一反应是「抽进 `packages/shared-view`」。不做，理由与该包 README
的归属判据一致：**文案是本地化的事，不是共享视图逻辑**。三端的 catalog 是三份独立的 `.po`
（桌面 zh/zh-TW/en、Web zh/zh-TW/en、移动 zh-Hans/en），locale 集合都不一样；共享一份
描述符表意味着三份 catalog 都要收同一批 msgid，反而把耦合做进了提取流程。

**共享的是 kind 的名字**，那已经由 Rust 侧的 `AppError` → `FfiError` / `WebError` 映射钉死了。

## D5 —— hex 收进 `crates/invite` 而不是新建 util crate

5 份拷贝处理的是同一个类型：`InviteRecord::capability_hash: [u8; 32]`，定义在
`crates/invite/src/store.rs`。函数跟着类型走。

`crates/invite` 是 wasm-clean、零 core 依赖的，5 个调用点（web × 2、storage-sql、src-tauri、
mobile-core）全都已经依赖它 —— 收口不引入任何新依赖边。

**不做成泛型 `to_hex<const N: usize>`**：只有一个用处，且 `[u8; 32]` 这个具体长度正是
「长度必须是 64 个 hex 字符」这条校验的依据。泛型化会把这条校验也变成运行时的。
