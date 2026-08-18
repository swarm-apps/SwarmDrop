## 1. 端口层重构与 native 实现拆分（先行，CLI 依赖其产物）

- [x] 1.1 通读 `src-tauri/src/host/identity_store.rs`，逐项判定哪些逻辑平台中立、哪些是桌面特有（路径解析、Tauri 类型），产出一份归属清单
- [x] 1.2 在 `crates/host` 新增身份与已配对设备的文件实现模块，整模块 `#[cfg(not(target_family = "wasm"))]`、IO 用同步 `std::fs`（对齐 `device_config_file.rs` 的既有约定）
- [x] 1.3 实现保持四条既有保证：密钥材料与设备列表分文件、写入原子（临时文件 + rename）、unix 0600、**读取失败以错误上报且不覆盖原文件**
- [x] 1.4 迁移两条护栏测试（原子写、读取失败不降级）到新位置，保持断言语义不变
- [x] 1.5 改造 `src-tauri/src/host/identity_store.rs` 为「提供路径 + 复用共享实现」，删除其重复逻辑
- [x] 1.6 跑 `cargo test --workspace`，确认桌面身份存储相关测试全绿
- [x] 1.7 跑 `./scripts/check-wasm.sh --clippy`，确认 `crates/host` 的新模块未破坏 wasm 双 target 门禁

### 1B —— native 实现拆分（实施期发现第三、四个共享实现后追加，见 design D8）

- [x] 1.8 新建 `crates/host-fs`（native-only，整 crate `cfg(not(target_family = "wasm"))` 语义），加入 workspace members
- [x] 1.9 把 `identity_store_file` 与 `device_config_file` 从 `crates/host` 移入 `host-fs`，连同各自的测试与 `tempfile` / tokio `sync` 依赖
- [x] 1.10 把桌面 `file_source/path_ops.rs` 与 `file_sink/path_ops.rs`（合计 616 行、17 条测试）移入 `host-fs`，**行为逐字不变**
- [x] 1.11 在 `host-fs` 内提供 `FileAccess` 的本地路径实现，桌面 `file_source.rs` / `file_sink.rs` 改为复用它并**删除未使用的 `_app` 参数**
- [x] 1.12 `crates/host` 回归纯端口：确认零文件 IO、移除 `tempfile` 与 tokio `sync` 依赖
- [x] 1.13 更新宿主依赖与引用：`src-tauri` / `mobile-core` 各自直接依赖 `host-fs`（不再经 core 的 re-export 取实现）
- [x] 1.14 确认 **`crates/core` 不依赖 `host-fs`**，`./scripts/check-wasm.sh --clippy` 仍过
- [x] 1.15 全量验证：`cargo test --workspace` 全绿（含迁移过来的 17 + 8 + 2 条测试）、fmt、clippy

## 2. CLI crate 骨架

- [x] 2.1 新建 `crates/cli`（package 名与 bin 名分别定为可支撑 `dist` 标签解析与干净命令名的组合），加入根 workspace members
- [x] 2.2 建立 `cmd/` `runtime/` `adapter/` `render/` 四个模块目录与分层约束注释（`cmd/` 不含网络与存储细节、`runtime/` 不含用户文案、`render/` 不含业务判断）
- [x] 2.3 接入参数解析，落地命令面骨架：`start` / `stop` / `status` / `pair` / `devices` / `send` / `inbox`，全部先返回未实现
- [x] 2.4 实现 `adapter/paths.rs`：解析 CLI 自己的数据目录，支持显式覆盖
- [x] 2.5 定义退出码枚举：成功、用法错误、节点未就绪、对端不可达、传输失败、被中止

## 3. 端口实现与组装

- [x] 3.1 接入 `host-fs` 的 `FileAccess` 本地路径实现（**不重写**），CLI 只提供保存位置：默认 `~/Downloads/SwarmDrop`，可配置
- [x] 3.2 实现 `adapter/events.rs`：事件订阅逻辑写一遍，渲染经渲染器 trait 分派
- [x] 3.3 实现 `runtime/boot.rs`：凑齐 `HostPorts`（`notifier` 传 `None`、`invite_store` 用既有 SQL 实现）并调用 `start_node`
- [x] 3.4 接上 `TransferManager`：注入事件 sink、存储与 `FileAccess`
- [x] 3.5 首启生成身份与默认设备名（默认名需能与同机图形界面宿主区分）
- [x] 3.6 冒烟验证：进程能起节点、能打印本机节点标识与监听地址后退出

## 4. 单实例仲裁与本地通道

- [x] 4.1 实现「发现」：通道存在且可连 ⇒ 有活节点；不可连 ⇒ 判为陈旧残留
- [x] 4.2 实现「仲裁」：判为陈旧后经数据目录文件锁裁决，拿到锁者清理残留并启动，未拿到者回到发现步骤重连
- [x] 4.3 实现通道服务端（类 Unix 用域套接字、Windows 用命名管道），载荷用长度前缀的结构化消息
- [x] 4.4 实现通道客户端，动词集与命令面一一对应（devices / send / status / inbox / stop）
- [x] 4.5 测试：重复 `start` 被拒绝且不启动第二个节点
- [x] 4.6 测试：进程被强杀后遗留的陈旧通道不阻塞下一次 `start`
- [x] 4.7 测试：两个进程同时判定残留失效时，只有一个成功启动

## 5. 节点生命周期命令

- [x] 5.1 `start` 前台模式：阻塞运行，收到终止信号后有序关闭
- [x] 5.2 `start` 后台选项：节点就绪后立即返回，节点继续运行
- [x] 5.3 `stop`：经通道请求关闭；无节点在运行时以成功退出并说明当前已停止
- [x] 5.4 `status`：输出节点状态、节点标识、监听地址、NAT 状态与中继可达性
- [x] 5.5 临时节点：无常驻节点时的一次性命令自起节点，命令结束销毁，结束后状态回到停止
- [x] 5.6 `stop` 对临时节点同样生效（显式意图优先于正在执行的命令）
- [x] 5.7 验证不存在任何隐式常驻启动路径（一次性命令绝不留下常驻节点）

## 6. 操作命令

- [x] 6.1 `pair` 生成：输出邀请链接 + 终端二维码，并提供关闭二维码的选项
- [x] 6.2 `pair <邀请>`：完成配对握手，成功后对端出现在设备列表
- [x] 6.3 `devices`：列出已配对设备及其在线状态
- [x] 6.4 `send`：按设备选择目标并发送文件或目录，进度可见
- [x] 6.5 `send` 的中止语义：用户中断时按既有续传语义处理，不留下损坏状态
- [x] 6.6 `inbox`：列出、查看与导出收件箱条目
- [x] 6.7 常驻节点存在时，以上命令全部经通道复用该节点（不启动第二个节点）
- [x] 6.8 验证被动接收：节点常驻期间对端发来的文件按既有接收策略落地，无需任何接收命令

## 7. 输出模式

- [x] 7.1 人类可读渲染：进度与状态写 stderr
- [x] 7.2 机器可读模式：结构化结果写 stdout
- [x] 7.3 验证结构化模式下 stdout 可被完整解析、进度信息不混入
- [x] 7.4 验证每个失败类别返回约定的退出码

## 8. 分发

- [x] 8.1 确定 npm 组织与包名（design.md 的 Open Question），记录到配置注释
- [x] 8.2 编写 `dist-workspace.toml`：设 `tag-namespace`、四种 installer、六个平台目标、复用既有 homebrew tap、关闭内建自更新
- [x] 8.3 在配置注释中记录发布标签必须使用斜杠形式的原因（namespace 与包名不同，连字符形式会被整串当版本号解析）
- [x] 8.4 生成发布 workflow，**核对既有 `.github/workflows/release.yml` 内容逐字未变**
- [x] 8.5 核对标签触发模式：CLI 标签不触发桌面与移动流水线，反之亦然
- [x] 8.6 同步 `CLAUDE.md`：Version management 增补第三条版本线，Workspace 布局表增补新 crate
- [ ] 8.7 试发一个预发布版本，验证每个受支持平台都有产物 —— **「每个平台都有产物」已本地验证**（`dist build --artifacts=all` 六平台齐全 + 独立交叉编译 6/6）；只差推 tag 真实发布，按 2026-08-19 决定推迟

## 9. 验收

- [ ] 9.1 干净机器验收：一台从未安装过 SwarmDrop 的机器，经任一渠道安装后完成 `pair` 与 `send` —— **干净环境已验证**（全新 HOME + 清空环境变量，起节点与 `pair` 均正常）；`send` 需第二台设备，installer 下载需真实 release
- [ ] 9.2 npm 渠道验收：另一个包把 CLI 声明为依赖并按平台解析后可执行到二进制 —— **包结构与下载地址已验证**（无 scope 包名、bin 映射、平台矩阵、URL 含 tag namespace）；只差发到 registry 再装一次
- [x] 9.3 并存验收：同机桌面端与 CLI 同时运行，两者以不同设备标识出现且互不干扰
- [x] 9.4 逐条走查 `specs/cli-host` 与 `specs/cli-distribution` 的全部 scenario
- [x] 9.5 跑全套机器门禁：`cargo fmt --all` / `cargo check --workspace --all-targets` / `cargo test --workspace` / `cargo clippy --workspace` / `./scripts/check-wasm.sh --clippy`
- [x] 9.6 补充 `dev-notes/knowledge/` 中与 CLI 相关的实践与坑（尤其单实例仲裁与分发标签形态）
