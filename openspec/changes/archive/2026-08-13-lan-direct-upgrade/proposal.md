## Why

**同一个局域网里的两台已配对设备，会一直经公网中继传文件。** 用户实测：手机与桌面在同一
Wi-Fi 下配对完成，设备卡片的连接徽标始终是「中继」。

这不是显示错了，是真的走了中继。两个互相独立的缺口，任意一个都足以造成这个结果：

### 缺口 1 — 移动端的 mDNS 被平台挡死

内核这边是开着的（`presets::Native` → `.mdns(true)`，移动端走同一个 profile），平台侧两个必需
项都没配：

- **iOS**：`mobile/app.json` 的 `ios` 只有 `bundleIdentifier`，没有 `infoPlist`。iOS 14 起本地
  网络访问要授权，**没有 `NSLocalNetworkUsageDescription` 连权限弹窗都不会出现**，组播被系统
  静默丢弃；`NSBonjourServices` 也得声明 libp2p 用的 `_p2p._udp`。
- **Android**：`AndroidManifest.xml` 有 `CHANGE_WIFI_STATE`，但没有 `CHANGE_WIFI_MULTICAST_STATE`，
  全仓也搜不到 `MulticastLock`。不持锁时多数 Wi-Fi 芯片在省电态把组播帧丢在驱动层。

结果：桌面的 `address_book` 里根本没有手机的私网地址，`connect` 只能拿 DHT/circuit 候选。

### 缺口 2 — relay → LAN 没有升级路径

就算 mDNS 修好了，这个还在：

- `actor.rs` 的 `try_upgrade_to_direct` 只在对端 identify 的 `listen_addrs` 里
  `find(is_webrtc)` —— **对端自报的私网地址被整个忽略**；
- mDNS `Discovered` 只 `record_addr` + emit 事件，**不拨号**；
- `handle_connect` 开头「已连接就返回当前快照」，presence 后续的 `connect` 不会重拨。

于是**谁先建成谁定终身**。presence 经 DHT 发现在线通常比 mDNS 先到，relay 先赢，之后永远 relay。

### 附带：链路信息在上层压根拿不到

内核的 `ConnInfo` 里有 `addr: Addr`，但没往上透。`Device` 只有 `connection`（lan/dcutr/relay）
+ `latency`。用户看到「中继」之后没有任何下一步——经的哪台 relay、跑在什么传输上、走的哪条地址，
一概不知，无从判断是网络问题还是配置问题。

## What Changes

**修复顺序刻意是「先内核后平台」**：缺口 2 的修复不依赖 mDNS，因此它同时也是缺口 1 的兜底——
即使 iOS 的组播最终被 Apple 的 multicast entitlement 拦住（见 design.md 的风险一节），局域网
直连仍然成立。

1. **内核：LAN 直连升级**（`crates/net`）。新增 `try_upgrade_to_lan`：当某 peer 的连接全是
   `Relayed`，且拿到它的私网地址时主动拨号升级为 `PathKind::Local`。地址两个来源——identify 的
   `listen_addrs`（**不依赖 mDNS**）与 mDNS `Discovered`。
2. **内核：mDNS 初始化失败不再 panic**。`Behaviour::new` 里的 `.expect("mDNS initialization failed")`
   把一个平台可选能力做成了启动硬前提；绑不上 5353 的环境会在节点启动时直接崩。
3. **链路详情透出**：`Addr::transport()` / `Addr::relay_node_id()`（net-base）→ `NetEvent` 携带
   `addr` → `Device.connectionDetails`（host）→ 三条 codegen → 三端 UI。
4. **移动端 mDNS 平台配置**：iOS `infoPlist`；Android `CHANGE_WIFI_MULTICAST_STATE` +
   一个只做 MulticastLock 的 expo module。
5. **三端 UI**：连接徽标可展开为链路详情（桌面/Web popover、移动就地展开），`DESIGN.md` 的
   Device Card Contract 补 slot 6 的披露条款。

## Impact

- **受影响 spec**：`lan-direct-connection`（新增）、`device-presentation-contract`（补链路详情披露）
- **受影响代码**：`crates/net-base`（+`TransportKind`、两个 `Addr` 谓词、可选 `specta` feature）、
  `crates/net`（升级路径 + 事件字段 + mDNS 降级）、`crates/host`（`ConnectionDetails`）、
  `crates/core`（`ConnectionSnapshot` 收口）、三端 UI 与三条 codegen 产物
- **行为变化（用户可见）**：
  | | 之前 | 之后 |
  |---|---|---|
  | 同 LAN 已配对设备 | 徽标长期停在「中继」 | 几秒内升级为「局域网」 |
  | 连接徽标出现时机（桌面） | 要等第一次 ping（**最多 30 秒**） | 连上即出，延迟随后补上 |
  | 徽标内容 | 局域网 / 打洞 / 中继 + 延迟 | 同上，另加传输名（TCP / QUIC / …） |
  | 点击徽标 | 无反应 | 展开链路详情（可复制 multiaddr） |
- **不做**：修 `DESIGN.md` 里已记录的桌面 Known gap（信任徽标与连接徽标三元互斥）——那是独立
  变更，本次只在同一片代码里路过，不顺手改布局。
