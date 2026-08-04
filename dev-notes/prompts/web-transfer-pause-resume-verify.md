# 任务：把 Web 端「暂停 → 续传」端到端验完

> **这是给新会话的启动提示词。** 直接把本文件路径丢给 Claude Code 即可开工。
>
> 前置阅读（按顺序，别跳）：
> 1. `CLAUDE.md` — 架构唯一事实源
> 2. `dev-notes/knowledge/web-app-frontend.md` — **「本机跑双节点」整节必读**（本文的坑在那里有归因）
> 3. `crates/web/src/store.rs` 的模块注释「落库范围」表 — 决定了发送方向能续到哪一步
>
> 开工前先调 `/dev-workflow`（项目硬性要求）。

---

## 一句话目标

把 PR #115 合并进 `develop` 但**没有跑过**的那条路径验完：Web 端暂停一条传输，再续上。

## 现状：代码在，验证欠着

`crates/web` 已导出 `pause_send` / `pause_receive`（PR #115，commit `31d844f2`），
传输页详情侧有「暂停」按钮，暂停后转 `suspended(LocalPaused)` 且 `recoverable = true`，
「续传」按钮随即出现。全量门禁绿：`cargo check --workspace` / `check-wasm.sh --clippy` /
`test-wasm.sh` / `tsc` / `vitest` / `next build` / `check:zustand-access`，CI 三个 job 也绿。

**但没有真的传一次文件再暂停再续上。** 暂停这条链路的风险不在编译期：

- `initiate_resume` 是一次**协议往返**——ResumeProbe → 校验 report → 注册新 epoch actor →
  ResumeCommit → dispatch `ResumeCommitted` → spawn 数据面。静态审查覆盖不到时序与
  epoch 守卫。**这是本任务真正的风险所在。**
- 发送方向恢复要**重建 SenderActor**（`build_sender_actor_for_resume`）：从 DB 行重建
  `prepared_files`。

> ⚠️ **2026-08-04 更正**：本文早先把「outboard 缺失时按源文件重算」列为最大未验风险，
> 说那条路径在浏览器上没跑过。查证后不成立——那段代码有 `if pf.outboard.is_empty()` 守卫，
> 而 `flow/send.rs` 在发送启动时就 `save_file_outboard`、Web 实现是写内存，同页面暂停时
> outboard 一直在。**同页面续传根本不会走重算路径**，验证时不必围着它转。

推导（为什么浏览器上恢复得了、以及恢复不了的那一半）写在 `crates/web/src/node.rs`
的 `pause_send` 文档注释里，先读它再动手。

## 环境：本机双节点（已趟通，照抄即可）

浏览器之间不能直连（浏览器不 listen socket），必须经 relay circuit。公网 bootstrap
在开发机上不一定连得通（实测 webrtc-direct 超时），所以起一个本地的。

### 1. 本地 relay

```bash
# ⚠️ external-ip 必须是本机局域网 IP（`ipconfig getifaddr en0`），不能是 127.0.0.1
cargo run -p swarm-bootstrap -- run \
  --external-ip 192.168.x.x --listen-ip 0.0.0.0 \
  --tcp-port 14001 --quic-port 14001 --webrtc-port 14003 \
  --key-file /tmp/relay/identity.key --webrtc-cert-file /tmp/relay/webrtc.pem
```

> **2026-08-04 更正：本文早先写的 `--external-ip 127.0.0.1` 配对走不通。**
> `select_invite_addrs`（`crates/core/src/pairing/manager.rs:50`）第一步就把
> `is_loopback_or_unspecified()` 的地址整条丢弃，而 circuit 地址的外层就是 relay 的 IP
> ——relay 挂在 loopback 上，A 的三条 circuit 地址会被全部过滤，**邀请里一个地址都不剩**。
> 症状是 B 侧报 `Network error: 发送配对请求失败: dial failed: Dial error: no addresses
> for peer.`，而两端 `relay_ready` 都是 true、circuit 也确实建起来了，极易误判成 relay 挂了。
> 一眼可判的自检点：**邀请串长度**。带地址的约 600 字符，不带的只有 ~230。

首次编译几分钟。日志里 grep「已公告公网地址」拿 webrtc-direct multiaddr，**末尾补
`/p2p/<relay 的 PeerId>`**（PeerId 在启动日志里，`12D3Koo…`）。

> 别用管道截断它的输出（`| head -n`）——`head` 收够行数就关管道，relay 会吃到
> SIGPIPE 直接死掉，看起来像「启动失败」。

### 2. 让两端只用这个 relay

**不必改源码**，`relay-helpers.ts` 留了环境变量：

```bash
cd docs && NEXT_PUBLIC_SWARMDROP_WEB_RELAY_HELPERS="<上一步那条完整 multiaddr>" pnpm build
```

### 3. 静态产物起两个端口 = 两个 origin

```bash
python3 -m http.server 3010 -d out &
python3 -m http.server 3011 -d out &
```

端口不同即不同 origin，localStorage 隔离，两个标签页因此是**两个独立身份**。
两个都是 `localhost`，满足 secure context（OPFS + WebCrypto 都要）。

### 4. 配对 → 发文件

A（`:3010`）设置页填 relay → 「建立可达（circuit）」→ 设备页「生成邀请」→ 复制链接；
B（`:3011`）设备页粘贴 → 「确认配对」。之后 A 发一个**够大的文件**（几十 MB，
否则来不及点暂停），B 接受。

自动化时几个可用的手法（这轮实测有效）：

- 邀请链接不在 DOM 里，拦 `navigator.clipboard.writeText` 再点「复制链接」能取到。
- React 受控 input 要用 native setter + `dispatchEvent(new Event('input',{bubbles:true}))`，
  直接赋 `.value` 不触发 onChange。

## 已知坑（都实测踩过，不是推测）

### 不能用 `pnpm dev` 跑双节点

Next dev server 拦跨 origin 访问：`127.0.0.1:3000` 那一侧**页面根本不 hydrate**。
症状极具误导性——UI 渲染正常（那是 SSR HTML）、按钮都是 disabled（服务端渲染时
`ready=false`）、console 干净无报错、节点永远停在「未启动」，而
`performance.getEntriesByType('resource')` 里 **wasm 一个字节都没 fetch**。
很容易误判成 wasm 坏了。必须用静态产物。

### 邀请会带上**所有** listen 地址

A 若同时连着公网 relay，邀请里就有两条 circuit 地址，对端会先拨公网那条并失败
（实测报 `Unexpected peer ID <relay 的 id> at <整条 circuit 地址>`）。第 2 步那个
环境变量把 helper 收敛成本地一条之后，邀请串会明显变短——**那是配对能走通的前提**，
也是一眼可判的自检点。

## 验收标准

> **2026-08-04 已执行**（Chrome 双 origin，500 MB 文件，本地 relay）。下面的勾选是实测结果。

发送方向（A 暂停自己发出的会话）：

- [x] 点「暂停」后 A 侧 phase → `suspended`、suspendedReason → `local_paused`，
      「暂停」按钮换成「续传」。实测停在 `128.0 MB / 500.0 MB 26%`，
      日志 `Send session paused: session=8126ea46…`
- [ ] ~~B 侧同一会话也转 suspended，reason 是 `remote_paused`~~
      **未通过**：B 转的是 `interrupted`（UI 显示「连接中断」）。B 的 console 里
      没有任何暂停通知，只有
      `WARN swarmdrop_net::router: protocol handler failed
      protocol=/swarmdrop/transfer-data/2 error=Transfer error: data channel 在完成前关闭`
      ——B 是靠数据通道被关自己推断的。详见下面「实测发现的缺陷」
- [x] 点「续传」后回到 `active`，**字节从断点继续而不是从 0 重来**：
      26% 暂停 → 续传后 46% → 直至 B 侧 `500.0 MB / 500.0 MB 100%`。
      日志 `发送方发起探测式恢复: session=8126ea46…`
- [x] 传完后文件内容正确：B 的 OPFS 里 `big5.bin` 大小 524288000 字节、
      sha256 `2abe04d7…496a04`，与源文件**逐字节一致**。断点拼接没有错位/重复/丢失
- [x] **刷新 A 的页面：非终态发送会话消失，只剩终态的**——预期行为不是 bug
      （非终态发送会话不落 IndexedDB）
- [x] 刷新前浏览器**弹出离开确认**（`ReloadGuard` 的 `beforeunload`）。
      验证手法：`beforeunload` 会被自动接受、看不到弹窗，改用合成事件断言
      `const ev = new Event('beforeunload', {cancelable: true});
      window.dispatchEvent(ev); ev.defaultPrevented`。
      **同一页面同一次加载**下的对照：只有终态会话 → `false`；发出一条非终态发送会话后
      → `true`。⚠️ 测完记得刷新页面清掉自己注册的探针监听器，否则下一轮的
      `defaultPrevented` 是被自己污染的
- [x] 只有接收/终态会话时**不拦截**（B 侧实测 `false`）——接收方向续得上，拦截纯打扰
- [x] 传输**进行中**（不只 suspended）详情侧就显示「这条发送只能在本页完成…」提示条；
      `waiting_accept` 阶段同样显示

接收方向（B 暂停自己在收的会话）：

- [ ] 暂停 → 续传同样走通
- [ ] **暂停后刷新 B 的页面：会话仍在**（suspended 的接收会话 `worth_persisting`），
      「续传」可点且能续上——OPFS 里的半成品与 checkpoint 都还在

## 实测发现的缺陷（2026-08-04，**当天已修**）

> 两条都已定位根因并修复，附回归测试；下面保留现象与推导。修复要点入库
> [`rust-backend.md`](../knowledge/rust-backend.md)。
>
> - **1** → `pause_send` / `pause_receive` 把 `notify_pause` 提到 cancel actor 之前。
>   锚点测试 `e2e_interrupted_first_shuts_out_late_remote_paused`。
> - **2** → `SenderActor::on_completed` 与 `on_interrupted` 对称地先落进度再转终态。
>   锚点测试在 `e2e_single_file_transfer` 尾部（回退修复后确实红：`left: Some(0)`）。
>
> **浏览器复验已完成**（2026-08-04 当天，重建 wasm + 双 origin + 500 MB）：
>
> - 暂停后 A「本机暂停 154.3 MB / 500.0 MB 31%」，**B「对方暂停 149.0 MB / 500.0 MB 30%」**
>   ——修复前这里是「连接中断」；
> - 续传 31% → 68% → 完成，**两侧都是「已完成 500.0 MB / 500.0 MB 100%」**
>   ——修复前发送方是「0 B / 0%」；
> - 接收文件 sha256 `37bd7e58…ce625`，与源文件一致。
>
> ⚠️ 改了 `crates/transfer` 之类被 `crates/web` 依赖的 crate 后，**必须先
> `pnpm build:wasm` 再 `pnpm build`**，否则浏览器加载的还是旧 wasm，验的是修复前的代码。

### 1. `notify_pause` 没到对端，接收方被显示成「连接中断」

发送方暂停后，接收方 console 里**没有任何暂停通知**，只有数据通道被关的 warn。于是 B 侧
`suspendedReason` 落成 `interrupted`、UI 显示「连接中断」，而不是 `remote_paused`「对方暂停」。

后果不是显示不精确而已：`SUSPENDED_LABEL` 那条注释自己说过「说成中断会让用户以为出了故障，
转头去查网络」——这里正好踩中。用户看到「连接中断」会去排查 WiFi，实际上对面只是按了暂停。

排查从发送侧的 `notify_pause` 调用点开始，确认它是否真的发了帧、以及接收侧
`transfer-data` 协议处理器是不是在收到暂停帧之前就因通道关闭而退出了。

### 2. 发送侧终态 projection 的 `transferredBytes` 不回填

传完之后 A 侧列表显示 `big5.bin 已完成 发送 0 B / 500.0 MB 0%`，而 B 侧同一条是
`500.0 MB / 500.0 MB 100%`。文件本身是好的（hash 一致），纯显示问题：发送方向的会话进终态时
没把最终字节数写回 projection，于是 `transferSample` 的「终态一律以 projection 为准」拿到的是
一个从没被更新过的 0。三条会话（120 MB / 300 MB / 500 MB）无一例外。

## 失败时去哪看

- 浏览器 console：内核 tracing 全打在这里（`swarmdrop_transfer` / `swarmdrop_net`），
  grep `resume` / `epoch` / `ResumeProbe` / `ResumeCommit`
- relay 进程日志：circuit 建立与断开
- 若 resume 报「会话状态不支持恢复」→ 看 `load_resumable_session` 的两个条件
  （`phase == Suspended && recoverable`）
- 若报「会话不存在」→ 多半是刷过页面了（发送方向），见上面的预期行为

## 验完之后

把结论补进 `dev-notes/knowledge/web-app-frontend.md`——**尤其是不符合预期的部分**。
如果发送方向的 resume 在浏览器上实际走不通（比如 outboard 重算路径炸了），
那不是「验证失败」而是一条要记下来的事实：届时应当把发送方向的「暂停」按钮收掉，
只留接收方向，并在这里写清为什么。
