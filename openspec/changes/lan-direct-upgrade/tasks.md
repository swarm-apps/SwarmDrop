# 任务

## 1. net-base：链路判定谓词

- [x] 1.1 `TransportKind` 枚举（`status.rs`，camelCase 序列化契约测试）
- [x] 1.2 `Addr::transport()` —— WebRTC 两个变体先判（打洞地址的 `/webrtc` 在 circuit 段之后）
- [x] 1.3 `Addr::relay_node_id()` —— 取 circuit 段**之前**的 `/p2p/`，与 `p2p_node_id()` 互补
- [x] 1.4 可选 `specta` feature（默认关闭，仅桌面 IPC 导出需要）
- [x] 1.5 单测：三类地址各自的 transport、两个 node_id 取到不同段

## 2. net：LAN 直连升级

- [x] 2.1 抽 `only_relayed(peer)`（两条升级路径共同前提）
- [x] 2.2 `upgrading` 拆成 `upgrading_lan` / `upgrading_direct` + `clear_upgrade_marks`
- [x] 2.3 `try_upgrade_to_lan`：私网候选（`is_lan_candidate`）+ `PeerCondition::Always` + 候选上限
- [x] 2.4 接线两个来源：identify `listen_addrs`、mDNS `Discovered`
- [x] 2.5 单测：`is_lan_candidate` 排除 loopback / link-local / 公网 / circuit
- [x] 2.6 mDNS 初始化失败降级为 warn（不再 `expect` panic）

## 3. net → host → core：链路详情透出

- [x] 3.1 `NetEvent::PeerConnected` / `PathChanged` 携带 `addr`（与 `path` 取自同一次快照）
- [x] 3.2 `ConnectionDetails`（host）+ `Device.connectionDetails`
- [x] 3.3 `PeerInfo.conn_addr` —— 与 mDNS 证据 `addrs` **分开存**
- [x] 3.4 `ConnectionSnapshot` 收口两条 `Device` 构造分支
- [x] 3.5 三条 codegen 重生成：tauri-specta / wasm-bindgen+specta / uniffi

## 4. 移动端 mDNS 平台配置

- [x] 4.1 iOS `infoPlist`：`NSLocalNetworkUsageDescription` + `NSBonjourServices`
- [x] 4.2 Android `CHANGE_WIFI_MULTICAST_STATE`（config plugin + 已提交的 manifest）
- [x] 4.3 `modules/lan-multicast` expo module（MulticastLock，`setReferenceCounted(false)`）
- [x] 4.4 锁的生命周期绑到节点启停（iOS / 模块缺席时 no-op）

## 5. 三端 UI

- [x] 5.1 共享包 `transportLabel`（专有名词，不进 catalog）+ 单测
- [x] 5.2 桌面：`ConnectionBadge` 组件（徽标 + Popover 详情 + 复制）
- [x] 5.3 桌面：徽标出现条件去掉 `latency != null`
- [x] 5.4 Web：`ConnectionBadge` + 补 `popover.tsx` + `useCopyToClipboard` hook（invite-share 一并收编）
- [x] 5.5 移动：`ConnectionDetailsSection` 就地展开 + 徽标加 transport
- [x] 5.6 `DESIGN.md` 补 slot 6 披露条款与降级说明

## 6. 门禁

- [x] 6.1 `cargo fmt --check` / `check --workspace --all-targets` / `test --workspace`（49 target）
- [x] 6.2 `cargo clippy --workspace` 零告警
- [x] 6.3 `./scripts/check-wasm.sh` + `--clippy`
- [x] 6.4 三端 typecheck + 单测 + build
- [x] 6.5 `check:zustand-access` / `check:clipboard` / `check:shared-view`
- [x] 6.6 三端 `i18n:extract` + 补齐 en / zh-TW 译文（零 missing）

## 7. 真机验证（本机无第二台设备，留给用户）

- [ ] 7.1 iOS：本地网络权限弹窗是否出现、mDNS 是否真的收发到组播
- [ ] 7.2 Android：MulticastLock 生效后能否在多播域看到对端
- [ ] 7.3 两端：观察 `upgrading relayed connection to lan direct` 日志与随后的 `PathChanged` → `Local`
