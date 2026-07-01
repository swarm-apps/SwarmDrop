## 背景

SwarmDrop 当前的设备模型基本是二元状态：某个 peer 已配对或未配对。`PairedDeviceInfo` 存储 peer id、OS/设备快照和配对时间；`PairingManager` 持有已配对设备 map，`DeviceManager` 在生成设备列表投影时读取它。传输 offer 当前由接收方用户通过普通入站 offer 流程接受或拒绝。

在 `drop-inbox-and-transfer-activity` 之后，完成接收的内容会有一个安全落点：Drop Inbox。这使低摩擦自动接收成为可能，但前提是产品有清晰的信任边界。策略层需要回答：

- 这是我自己的设备、协作者设备、临时设备，还是被阻止设备？
- 这个 peer 是否可以不经显式确认发送？
- 如果自动接收，内容会落到哪里？
- 哪些限制保护接收方？

本设计优先处理接收策略。它会建立后续 MCP 权限也能复用的信任和策略模型，但不在本变更实现完整 MCP 权限 UI。

## 目标 / 非目标

**目标：**

- 新增持久化信任层级和每设备接收策略。
- 提供安全的默认策略模板。
- 将现有已配对设备迁移为需要确认的 collaborator 策略。
- 允许用户在配对后分类设备，并可后续编辑策略。
- 在收到入站 transfer offer 时执行策略。
- 将自动接收的内容送入收件箱，而不是直接进入任意最终内容界面。
- 在活动与恢复中暴露策略决策。

**非目标：**

- 不替换配对协议。
- 不新增账号级身份或跨设备所有权证明。
- 不实现完整 MCP 客户端权限管理。
- 不实现临时公开链接分享；只保留一次性/TTL 语义所需的策略字段。
- 不增加云端策略同步。

## 决策

### 1. 策略随已配对设备元数据存储，并提供迁移路径

扩展已配对设备持久化，增加版本化策略对象，而不是只建 DB 策略表。已配对设备当前通过 host `KeychainProvider` 加载，且在网络启动前就会消费；策略必须和配对事实同一时间可用。

建议共享结构：

```text
PairedDeviceInfo
  peer_id
  os_info
  paired_at
  trust_level: DeviceTrustLevel
  receive_policy: DeviceReceivePolicy

DeviceTrustLevel
  owned | collaborator | temporary | blocked

DeviceReceivePolicy
  auto_accept: bool
  require_confirmation: bool
  max_transfer_bytes: Option<u64>
  allow_directories: bool
  allow_relay_auto_accept: bool
  save_behavior: InboxOnly | InboxAndDefaultSaveLocation
  default_save_location: Option<CoreSaveLocation>
  allow_mcp_send_to_device: bool
  expires_at: Option<i64>
```

理由：运行时设备 map 和 secret/keychain 持久化已经围绕 `PairedDeviceInfo`。把策略放在这里，可以避免“keychain 里已配对，但策略在尚未加载的 DB 里”的双事实问题。

备选方案：策略只存 SQLite。v1 拒绝此方案，因为配对事实当前不是 DB-backed，而且网络启动会从 host identity storage 读取已配对设备。

### 2. 现有和新配对设备都默认使用安全 collaborator 策略

默认策略：

```text
trust_level = collaborator
auto_accept = false
require_confirmation = true
allow_directories = true
allow_relay_auto_accept = false
allow_mcp_send_to_device = false
save_behavior = InboxOnly
```

用户必须显式选择或后续编辑，才进入自有设备策略：

```text
trust_level = owned
auto_accept = true
require_confirmation = false
allow_directories = true
allow_relay_auto_accept = false by default
save_behavior = InboxOnly
```

理由：自动接收很方便，也有风险。默认值必须保持当前显式确认行为。

### 3. 策略决策发生在显示/自动接受入站 offer 之前

在 incoming offer 处理里增加策略评估：

```text
入站 offer
   |
   v
加载 peer 策略
   |
   +-- blocked ------------------> 带策略原因拒绝
   +-- temporary expired ---------> 带策略原因拒绝
   +-- violates size/dir/relay ---> 要求确认或拒绝
   +-- auto_accept allowed -------> 使用收件箱/默认保存行为接受
   +-- otherwise -----------------> 发出常规入站 offer 确认
```

策略评估返回：

- `AutoAccept`
- `RequireConfirmation`
- `Reject { reason }`

活动与恢复应记录或展示策略结果，让用户知道某次传输为什么自动开始或为什么被拒绝。

### 4. 自动接收必须落入收件箱

自动接收必须使用 `drop-inbox-and-transfer-activity` 提供的收件箱路径。

v1 推荐行为：

- 自动接收使用 `InboxOnly`。
- 物理文件位于普通配置的接收位置或受管理的 Inbox 接收目录，具体取决于收件箱变更最终的存储决策。
- 用户后续可以从收件箱显示、导出或另存。

理由：这保留了可检查的缓冲区。“自有设备自动接收”不应该等同于“无限制静默写入任意文件夹”。

### 5. 信任 UI 首先放在设备页，而不是设置页

信任是用户与某台设备的关系属性。主 UI 应位于设备卡片/详情/操作菜单：

- 已配对设备显示信任徽标。
- 配对后提示：“这是我的设备” vs “这是他人的设备”。
- 提供编辑策略入口。
- 提供阻止/解除阻止操作。

设置页后续可以提供全局默认值，但 v1 聚焦每设备策略。

## 风险 / 权衡

- **[风险] 用户误把他人设备标记为自有设备** -> 使用明确文案、可见信任徽标和易用的降级/阻止操作。
- **[风险] relay 自动接收意外消耗带宽** -> 默认 `allow_relay_auto_accept=false`；除非用户显式开启，否则 relay 路径需要确认。
- **[风险] Keychain JSON 迁移破坏旧已配对设备** -> 新字段反序列化时 optional/default，成功加载后再写回升级格式。
- **[风险] offer 到达时策略尚未加载** -> 缺失策略按 collaborator/需要确认处理。
- **[风险] shared core/RN binding 变动较大** -> 新增可序列化 enum/record 并版本化，桌面 UI 与移动端 UI 分阶段落地。

## 迁移计划

1. 给 `PairedDeviceInfo` 增加 optional/defaulted 策略字段。
2. 为旧已配对设备记录提供安全默认值。
3. 在 core 和 Tauri 增加策略更新/读取 API。
4. 在入站 transfer offer 中增加策略评估。
5. 增加桌面设备信任 UI 和配对后分类提示。
6. 增加策略默认值、迁移、自动接收、确认、拒绝路径测试。

回滚策略：由于旧记录默认 collaborator/需要确认，可以通过把所有设备按 collaborator 处理来停用策略执行，同时保留持久化字段不参与决策。

## 待确认问题

- 自有设备分类是否需要对端也确认？初步建议：v1 不需要，它是本地信任标签。
- 临时设备应该表示为带 `expires_at` 的已配对设备，还是独立的分享码身份？初步建议：如果 peer 已知，v1 复用 paired-device policy；真正未配对的临时分享链接留给后续变更。
- 自动接收应该使用受管理的 Inbox 目录还是用户当前默认接收目录？这取决于收件箱最终存储决策；策略必须调用收件箱/save behavior，而不是独立选择路径。
