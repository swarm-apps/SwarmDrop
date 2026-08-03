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
  epoch 守卫。
- 发送方向恢复要**重建 SenderActor**：从 DB 行重建 `prepared_files`，outboard 缺失时
  按源文件重算（`build_sender_actor_for_resume`）。浏览器上源文件是 `File` 句柄，
  这条重算路径没被跑过。

推导（为什么浏览器上恢复得了、以及恢复不了的那一半）写在 `crates/web/src/node.rs`
的 `pause_send` 文档注释里，先读它再动手。

## 环境：本机双节点（已趟通，照抄即可）

浏览器之间不能直连（浏览器不 listen socket），必须经 relay circuit。公网 bootstrap
在开发机上不一定连得通（实测 webrtc-direct 超时），所以起一个本地的。

### 1. 本地 relay

```bash
cargo run -p swarm-bootstrap -- run \
  --external-ip 127.0.0.1 --listen-ip 127.0.0.1 \
  --tcp-port 14001 --quic-port 14001 --webrtc-port 14003 \
  --key-file /tmp/relay/identity.key --webrtc-cert-file /tmp/relay/webrtc.pem
```

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

发送方向（A 暂停自己发出的会话）：

- [ ] 点「暂停」后 A 侧 phase → `suspended`、suspendedReason → `local_paused`，
      「暂停」按钮换成「续传」
- [ ] B 侧同一会话也转 suspended（`notify_pause` 通知到位），reason 是 `remote_paused`
- [ ] 点「续传」后两侧回到 `active`，**字节从断点继续而不是从 0 重来**
      （看 `transferredBytes` 与逐文件进度）
- [ ] 续传后 `epoch` 递增（事件日志里看 projection）
- [ ] 传完后收件箱条目正常、文件可下载且内容正确（比对 hash）
- [ ] **暂停后刷新 A 的页面：会话消失，不给「续传」按钮**——这是预期行为不是 bug
      （非终态发送会话不落 IndexedDB，详情侧那句提示说的就是它）

接收方向（B 暂停自己在收的会话）：

- [ ] 暂停 → 续传同样走通
- [ ] **暂停后刷新 B 的页面：会话仍在**（suspended 的接收会话 `worth_persisting`），
      「续传」可点且能续上——OPFS 里的半成品与 checkpoint 都还在

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
