# Agent 沙箱 —— 给 agent 加一层内核级的真相与约束

> 日期：2026-08-05（构想与否决同日）
> 状态：🔴 **已放弃 —— 保留论证**
> 否决理由：**方向判断是对的，但晚了约五个月。** 每一层都已有活跃的开源实现，
> 商业层面 a16z 的 $20M 已经进场。
> 与 SwarmDrop 的关系：独立项目构想，见 [`../../ai-era-product-directions.md`](../../ai-era-product-directions.md)

---

## 0. 先看结论

原本的定位是：「不是再做一个沙箱，而是给已有的 agent 加一层来自内核的真相与约束。
跨 agent、本地优先、可观测第一。」

**这个定位现在有人做、有论文、有融资。** 2026-08-05 用 GitHub + Exa 查证的全景：

| 原方案的层 | 谁已经在做 | 星数 | 备注 |
|---|---|---|---|
| **隔离层**（Landlock+seccomp） | [multikernel/sandlock](https://github.com/multikernel/sandlock) | **320★** | Rust、Apache-2.0、**有 arXiv 论文**（2605.26298）。2026-03-13 创建，2026-08-04 仍在更新 |
| | kstenerud/yoloai | 182★ | "AI agent sandboxing done right" |
| | showkw/mimobox | 4★ | Rust，OS/Wasm/microVM 三层 + MCP server + macOS Seatbelt |
| | nmicic/compartment | 3★ | Landlock+seccomp+BPF-LSM，README 直接举例 `./compartment-user -- claude` |
| **观测层**（eBPF 内核真相） | ccfos/huatuo | **1052★** | eBPF observability for Linux kernel & Agent sandbox |
| | luisadrianpuga/sentinel | 0★ | Go，抓 `openat`/`connect`/`execve`/`write`，SQLite，**支持 session diff** |
| **界面层** | takattowo/claude-code-devtools | 1★ | live timeline + **file heatmap** + replay scrubber |
| | simple10/agents-observe | — | hook 驱动的实时 dashboard，支持 Claude Code + Codex |
| **元层面** | kajogo777/the-agent-sandbox-taxonomy | 95★ | **给 27 个沙箱打分的分类学框架** |

最后一行是最强的信号：**一个领域出现分类学和评分框架时，它早就过了「有没有人做」的阶段。**

**商业层面：**

- **Runta**（SF）——2026-07 拿 a16z 领投的 **$20M seed**，post-money >$100M，
  投资人含 Jeff Dean、Fei-Fei Li、Ali Ghodsi、Thomas Wolf。做 agent runtime 的隔离、
  策略边界与问责链路
- **Runtime**（YC P26，$500K，2026-03）——宣传语是 "sandboxed coding agents with
  guardrails and observability"，与本方案原定位几乎同义

### 三个最刺眼的事实

**1. Sandlock 就是本文第 2.1 节的完整实现，而且更深。**
它做了本方案没想到的：`seccomp` user notification + `pidfd_getfd` 处理运行时决策、
HTTP 级 ACL、copy-on-write 可逆文件系统效果、pipeline 分段限权。
性能数据：~5ms 启动开销，端到端比 Docker 快 44×。

**2. sentinel 的 README 第一句，与本方案的立论几乎逐字相同：**

> "application logs tell you what the agent says it did.
> `sentinel` gives you a kernel-observed record of what it actually did."

本方案曾把这句话当作独占洞察。它不是。

**3. 一个反直觉的负面信号，比「拥挤」更值得记住。**

看星数分布：**隔离层 320★ / 182★，观测与界面层 1★ / 0★。**

这不能读成「界面层是空白机会」。更可能的解释是：**用户要的是「关住它」，不是「看着它」。**
关住是刚需（怕删库），看着是好奇心（第一天新鲜，第三天不开了）。
claude-code-devtools 的 feature 全做完了、README 写得漂亮、1 个星。

> **「没人做」和「没人要」，在星数上长得一模一样。**
> 本方案第一版把前者当成了机会，这是最该记住的错误。

---

## 1. 立论（保留 —— 它是对的，只是不独占）

今天多数 agent 的安全模型是**意图层拦截**——「你要执行 `rm -rf build/` 吗？」。
两个不可修复的缺陷：

**意图不可穷举。** agent 生成的是自然语言和 shell，危险形态枚举不完。
`curl x | sh`、npm postinstall、python `os.system` —— 每个都是新形状，规则永远落后于生成能力。

**确认疲劳是必然终局。** 用户前两小时认真读，之后一律回车，最后开
`--dangerously-skip-permissions`。一个每分钟打断三次的安全机制，最终一定会被关掉。

正确的层是**能力层**：进程树能碰哪些路径、连哪些端点、调哪些 syscall。
那是内核的语言——可穷举、可验证、不依赖 agent 诚实。

**这条立论经受住了检验**——Sandlock 的论文摘要给出了几乎相同的问题陈述，
sentinel 的 README 也是。判断没错，只是它是一个已被多方独立发现的判断。

---

## 2. 原方案要点（压缩保留）

### 2.1 隔离层

| 机制 | 作用 | 谁已实现 |
|---|---|---|
| **Landlock** | 文件系统 ACL，不需要 root | Sandlock / MimoBox / compartment / yoloai |
| **seccomp-bpf** | syscall 白名单 | 同上 |
| **namespace** | mount / net / pid 视图隔离 | compartment 的 `sandbox.sh` |
| **microVM** | 高隔离档 | MimoBox（KVM）、pullrun（Firecracker，~400ms） |

原本的判断是「先做前三个，microVM 留第二档」。**这个判断本身是对的**——
Sandlock 走的正是这条路，且刻意不用 cgroups / images / 强制 namespace。

### 2.2 观测层

用 **aya**（纯 Rust eBPF，不依赖 libbpf/C）挂 tracepoint 与 LSM hook，抓
`openat` / `unlinkat` / `renameat` / `connect` / `sendto` / `execve` + deny 事件。

sentinel 用 Go + eBPF 抓的是同一组 syscall，且已实现 session diff。

### 2.3 策略层三档

`profile` 预设 → `sandbox.toml` capability 声明 → **`learn` 模式**（只观测不拦截，
跑完从实际足迹反推最小策略）。

第三档曾被认为是杀手锏，理由是「AppArmor 和 SELinux 都死于策略没人愿意手写」。
**这个理由仍然成立，且未见有项目把它做成主打**——是原方案里少数没被直接覆盖的点。

### 2.4 界面层

足迹地图（空间视图而非日志表格）、进程树、网络面板、异步批准队列（不弹模态框打断）、
事后回放与两次 run 的 diff。

claude-code-devtools 已实现 file heatmap + replay scrubber，
但**数据源是 agent 自报的 transcript**，不是内核。

---

## 3. 唯一还空着的缝隙（窄，且不足以立项）

**内核流 × agent 语义的对齐。**

- sentinel 有内核真相，但**不知道哪个 syscall 属于哪次 tool call**
- claude-code-devtools 有 tool call 语义，但**数据源是 agent 自报的 transcript**

把两条流按时间对齐，输出「agent 说它读了 3 个文件，内核说 47 个」的 diff ——
这个交叉点确实还空着。

**但这是一个 feature，不是一个项目**，更不是一家公司。

**macOS 缺口是真的，但不是努力能解决的。** MimoBox 明说 Wasm / microVM / MCP server /
HTTP proxy 全部 Linux only，mac 只剩 Seatbelt。而 mac 恰是 coding agent 用户的主力平台。
缺口真实存在，但成因是 Apple 把 Endpoint Security Framework 锁在特批 entitlement 后面
——**这解释了为什么所有人都绕开，也说明绕不过去。**

---

## 4. 作废的部分

原路线图（M0 CLI → M1 eBPF → M2 策略 → M3 界面 → M4 macOS，共约 4 个月）**整体作废**。

原「开放问题清单」里的两道生死题，已被外部证据回答：

| 原问题 | 现在的答案 |
|---|---|
| 能力层约束会不会让主流 agent 工具链大面积崩溃？ | **不会。** Sandlock 跑 Redis 达到裸机吞吐（测量噪声内），5ms 启动开销 |
| eBPF 的 `CAP_BPF` / root 需求在桌面产品里怎么解？ | **绕开它。** Sandlock 全程 unprivileged——静态策略进 Landlock/seccomp，运行时决策走 seccomp notification supervisor，不需要 eBPF |

第二条尤其值得记：原方案默认「要内核真相就得上 eBPF」，
而 Sandlock 证明 seccomp user notification 能在**不要任何特权**的前提下拿到同等信息。
**这是一次纯粹的知识盲区，不是判断失误。**

---

## 5. 复盘：这次判断错在哪、对在哪

**对的部分：**

- 立论正确，且被学术界与 a16z 独立验证
- 技术选型正确（Landlock + seccomp + 非特权路径，正是 Sandlock 的路线）
- 识别缺口的能力可信——独立推导出的方向，五个月后有论文和 $20M

**错的部分（按严重程度）：**

1. **没有先搜就写方案。** 构想与否决同日发生，中间只隔一次 20 分钟的调研。
   这 20 分钟本该发生在写第一行之前。
   → **以后任何构想，动笔前先做一轮 GitHub + Exa 搜索。**
2. **把「没人做」直接读成「机会」。** 观测界面层无人涉足，被当作差异化写进方案，
   而星数分布指向的更可能是「没人要」。
   → **「没人做」是一个待解释的现象，不是一个结论。**
3. **知识盲区当成技术约束。** 认定 eBPF 是唯一路径，因而把 `CAP_BPF` 权限问题
   列为「产品上最难的一关」。实际存在不要特权的解法。

**结论不变的部分：**

原方案第 9 节判断「内核 6–8 周（rCore）的学习投入值得做」——**这条仍然成立，理由要换**：
不再是「为了做这个项目」，而是「为了能读懂 Sandlock 在干什么、能给它提 PR」。
给一个 320★ 的活跃 Rust 项目贡献 Landlock/seccomp 相关代码，
价值高于再造一个 4★ 的轮子——而这正是在 libp2p（#6558/#6560/#6576）和
webrtc-rs（五个补丁进 0.20.0）上已经验证过能做成的事。

**真正的独占位不在这里。** 通用 agent 沙箱要的是隔离深度，那是红海。
但「agent 在设备 A 上产生的东西，安全地送到设备 B」需要 P2P + 身份 + 沙箱三块同时具备
——SwarmDrop 已有前两块，而做沙箱的那批人一块都没有。**这个组合才是别人抄不动的**，
且它的落点在本仓，不需要另开项目。
