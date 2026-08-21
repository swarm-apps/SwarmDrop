## 1. 订阅面设备表的单一产出点（本仓，**已实现**）

- [x] 1.1 `runtime/watch/event.rs`：`invalidates_devices` → `affects_devices`，纳入
      `CoreEvent::DevicesChanged`；从 `translate` 移除该事件的翻译分支
- [x] 1.2 `runtime/watch/event.rs`：新增 `produces_frame`（`translate` 有 **或** 触发现取），
      `report_loss` 改问它——否则一次真实的名册丢失会被判成「路过」
- [x] 1.3 `runtime/watch/serve.rs`：`forward` 的判据换成 `affects_devices`，设备表只从
      `devices_now`（`DeviceFilter::Paired`）出
- [x] 1.4 `WatchEvent::DevicesChanged` 的文档注释写清「全量指的是全量*已配对*，且只能现取」，
      并点名内核那条事件的口径为什么不同
- [x] 1.5 两条护栏测试：`the_kernel_device_event_carries_no_table_of_its_own`、
      `a_dropped_device_event_is_reported`
- [x] 1.6 实测验证：一台已配对的离线设备，重启节点后订阅**不再**出现
      「baseline devices=1 → devicesChanged devices=0」

## 2. CLI 配置持久化底座

- [x] 2.1 在 `crates/cli` 新增私有配置模块：数据目录内一个 JSON 文件，装接收落点与引导节点的
      custom/removed 两个集合（设备名**不**搬进来，见 design D1）
- [x] 2.2 读写走**原子写**（临时文件 + rename）并在解析失败时返回 `Err` 而**不**回落默认——
      与 `JsonFileIdentityStore` 同一体例（静默回落会让用户的配置在一次坏块后消失）
- [x] 2.3 文件不存在 = 全部未设置，**不是错误**；首次写入时创建
- [x] 2.4 单测：往返读写、缺文件、损坏文件报错、两个 `--data-dir` 互不影响

## 3. `swarmdrop config`：标量设置

- [x] 3.1 `cmd/config.rs`：`list` / `get <key>` / `set <key> <value>` / `unset <key>`，
      key 是封闭集合（`device-name`、`receive-dir`）
- [x] 3.2 三层来源解析：`env` → `config` → `default`；`adapter/receive.rs` 的
      `resolve()` 接入持久化层，**保留** `SWARMDROP_RECEIVE_DIR` 的最高优先级
- [x] 3.3 `--json` 读面每项给 `key` / `value` / `source` / `configured`（被覆盖时是那个被压住
      的值）——设置界面靠它解释「为什么我改了没生效」
- [x] 3.4 写入前校验：设备名走 `DeviceName::parse` 同一入口；落点整串处理（**不**按 shell 拆
      分）、`~` 展开、可创建可写
- [x] 3.5 写入路径按 `RecordAccess` 分叉：有节点经 IPC，无节点直写；无节点时 **MUST NOT**
      为写入而拉起节点
- [x] 3.6 新增 IPC 动词：设置设备名（转调 core 的改名编排，**不**自己拼 `agent_version`）、
      设置接收落点（更新常驻节点内存里的落点，对此后的接收生效）
- [x] 3.7 写入结果如实回报「已生效」还是「待下次启动生效」，`--json` 下是结构化字段
- [x] 3.8 环境变量覆盖时写入仍然成功，但明确告知此刻不生效及原因
- [x] 3.9 测试：优先级三层、`unset` 回落到默认而非空值、环境变量存在时的写入回报、
      落点含空格不被拆分、无节点时不启动节点

## 4. `swarmdrop bootstrap`：引导节点集合

- [x] 4.1 `cmd/bootstrap.rs`：`list` / `add <addr>` / `remove <addr>`
- [x] 4.2 `runtime/bootstrap_nodes.rs`：内置常量保留，`default_network_config()` 改为
      `(内置 − removed) ∪ custom`
- [x] 4.3 增删的不对称语义（design D3）：撤销内置项写进 `removed`，撤销自定义项从 `custom`
      移除；**绝不**把内置清单复制进 `custom` 再编辑
- [x] 4.4 提交前同步校验：Multiaddr 可解析、含合法 `/p2p/`、传输协议在
      `Endpoint::supported_transports()` 内、与既有条目（含内置）不重复；**零网络往返**
- [x] 4.5 `--json` 列表每条标明 `builtin` / `custom` 与能否移除；被撤销的内置项不出现
- [x] 4.6 有节点时经 IPC 即时登记 / 撤销 infra 意图；无节点时只持久化，下次启动回放
- [x] 4.7 允许清空到零条（design D3），但 `status` 与 `list` 要让它显而易见
- [x] 4.8 测试：合并规则（含「升级换了内置地址、老用户拿到新的」这条回归）、增删不对称、
      校验拒绝的四类输入、无节点时不启动节点

## 5. 本仓门禁与文档

- [x] 5.1 `cargo fmt --all` / `cargo clippy` / `cargo test --workspace` / `./scripts/check-wasm.sh`
- [x] 5.2 `dev-notes/knowledge/cli-host.md` 补三节：配置面的三层来源、有节点必须走 IPC 的理由、
      引导节点两集合模型
- [x] 5.3 `crates/cli/CHANGELOG.md` + `crates/cli/Cargo.toml` 版本号；`CLAUDE.md` 的 CLI 版本行
- [ ] 5.4 发版走 `./scripts/release-cli.sh`（**不**手敲 `git tag`，理由见 CLAUDE.md）

## 6. 仓外 `dsh-swarmdrop`（`/Volumes/yexiyue/dsh-swarmdrop`）— 面板可信性，**已实现**

- [x] 6.1 `panel-port.ts`：全局 `busy` 改为按控件的 `BusyKey`（`node` / `pair` /
      `device:<peerId>`），一次没落定的调用不再禁用整块面板
- [x] 6.2 每次 RPC 带 deadline（state 35s / network 15s / action 45s，都在 Host 侧上界之上，
      因此是**传输层看门狗**而非动作时限）；超时文案不再是 `signal timed out`
- [x] 6.3 `cli.ts`：`StreamSpec.explain` 返回 `string | null`，**任何**非请求的退出都上报——
      `swarmdrop watch` 处理 SIGTERM 后退出码是 0，原来的 `code === 0` 豁免让订阅静默死亡
- [x] 6.4 新增 `subscription.ts`：受监督的 `watch`，指数退避（1s→30s）、**按帧**重置退避、
      dispose 后不再重连、已退出的进程不再 SIGTERM
- [x] 6.5 `StateAnswer.subscription`：订阅断开时面板顶部一条「信息可能是旧的」
- [x] 6.6 侧栏图标换成 SwarmDrop 品牌标记（`currentColor`，跟随主题与 hover）
- [x] 6.7 `subscription.test.ts` 10 条；全量 43 条通过
- [x] 6.8 实测：杀掉订阅进程 → 面板报「已断开」→ 自动重连 → 恢复为 `null`

## 7. 仓外 `dsh-swarmdrop` — 设置页批 1（只用现有 CLI 动词）

- [x] 7.1 `settings.section` 注册与页面骨架（`id: 'swarmdrop'`，locale 命名空间复用现有
      `swarmdrop`；组件只拿得到 `close()`）
- [x] 7.2 **邀请**一节：`invite list` + `invite revoke`（优先级最高——`pair-invite` 要求各端
      都提供清点与撤销，邀请 24h 有效且跨重启存活，泄露后这是唯一止损手段）
- [x] 7.3 **收件箱**一节：`inbox list` 完整列表（标题 / 来源 / 大小 / 时间 / 落盘路径 /
      missing）+ `inbox export`
- [x] 7.4 **节点与网络**一节：全量监听地址（面板只显示条数）、公网地址、NAT / 中继 / 引导，
      每条可复制
- [x] 7.5 **设备**一节：完整设备卡（名字 / 在线 / os·arch / 连接方式 / 节点标识）+ 解除
- [x] 7.6 **传输记录**一节：`transfer list` + 暂停 / 恢复 / 取消
- [x] 7.7 **关于**一节：插件版本 + CLI 版本 + 更新检查
- [x] 7.8 侧栏面板的「收件箱」那一行改为可点，**就地展开**最近几条（`openSection` 拿不到，
      跳转做不了，见 design D7）
- [x] 7.9 数据面：新增 RPC 端点，打开时取一次 + 手动刷新，**不轮询**；每个端点 spawn 一个
      CLI 进程这件事在注释里写明
- [x] 7.10 版本错配处理：识别 clap 的 `unexpected argument`，给「这项需要 swarmdrop x.y.z
      或更新的版本」，沿用 `explainPairingExit` 的做法

## 8. 仓外 `dsh-swarmdrop` — 设置页批 2（依赖第 3、4 节的新动词）

- [x] 8.1 **设备名**一行：读 `config get device-name`，写 `config set`，展示生效状态
- [x] 8.2 **接收落点**一行：展示 `value` / `source`；`source == "env"` 时输入框旁明确说明
      「当前被 `SWARMDROP_RECEIVE_DIR` 覆盖」，而不是让用户改一个不生效的框
- [x] 8.3 **引导节点**一节：列表（内置 / 自定义标记 + `InfraLink` 状态 + 失败原文）、
      添加、移除
- [x] 8.4 三项写入后的反馈区分「已生效」与「待下次启动生效」

## 9. 端到端验证与收尾

- [ ] 9.1 两台真机：改设备名 → 对端在**不重连**的情况下看到新名字
- [ ] 9.2 加一条不可达的引导节点 → 15 秒内在设置页转为失败态并带原文
- [ ] 9.3 改接收落点 → 此后收到的文件落在新位置，已收下的留在原处
- [~] 9.4 设置页在 dsh 里走过一遍（独立 profile + 独立数据目录）：七节全部渲染、六节的
      读路径逐个验过、引导节点移除与设备名写入在**界面上**验过、更新检查验过。
      **邀请撤销 / 收件箱导出 / 传输取消没验**——那台测试实例上这三样都是空的，
      要造出它们得先起节点并真配一次对，属于 9.1–9.3 那一档
- [x] 9.5 `dsh-swarmdrop` 的 CHANGELOG / README / `dev-notes/dsh-seams.md` 更新（版本已升到 0.3.0）；
      **npm 未发布**——与 CLI 的 tag 一样是对外动作，等一句确认
- [x] 9.6 清理本轮验证留下的痕迹（测试 profile、临时二进制、多余的 watcher 进程）
