## Context

SwarmDrop 的配对码系统目前使用 `DashMap<String, PairingCodeInfo>` 允许同时存在多个活跃码。实际用户流程中，用户每次进入配对页面只需要一个码，多码并行没有实际价值，却增加了后端状态管理和前端展示的复杂度。`handle_pairing_request` 中对 `active_codes.remove(code)` 的调用也依赖 map key 查找，改为单例后逻辑更直接。

## Goals / Non-Goals

**Goals:**
- 同一时刻最多一个活跃配对码
- 生成新码时自动覆盖（旧 DHT 记录靠 TTL 自然过期，新记录立即生效）
- 配对成功后自动消耗（置 `None`），前端可据此判断是否需要刷新
- 保持前端 API（`generate_pairing_code`、`respond_pairing_request`）签名不变

**Non-Goals:**
- TOTP 式时间滚动码（与 DHT key 推导机制冲突，成本高）
- 支持多设备同时扫码等待（非当前使用场景）
- 修改 DHT 发布/查询逻辑（key 推导方式不变）

## Decisions

### 1. 存储结构：`Mutex<Option<PairingCodeInfo>>`

**选择**：将 `active_codes: DashMap<String, PairingCodeInfo>` 替换为 `active_code: Mutex<Option<PairingCodeInfo>>`。

**理由**：`DashMap` 是为高并发读写设计的，但配对码操作（生成、验证、消耗）本身是串行的低频操作，普通 `Mutex` 足够且语义更清晰。`Option` 直接表达"有码"/"无码"两种状态，无需 key 查找。

**备选**：`RwLock<Option<...>>` —— 读多写少场景有优势，但配对码读写频率极低，差异可忽略。

### 2. 旧码 DHT 记录的处理

**选择**：生成新码时，不主动删除旧码的 DHT 记录，依赖 TTL（300s）自然过期。

**理由**：
- 主动删除需要记住旧 key（`SHA256(old_code)`），需要额外存储
- 旧码在内存中已被覆盖，即使对方查到旧 DHT 记录，`handle_pairing_request` 也会因内存中无对应码而拒绝
- TTL 300s 足够短，不会造成实质性安全问题

### 3. 配对成功后的码状态

**选择**：`handle_pairing_request` 接受配对后，将 `active_code` 置为 `None`（消耗码）。

**理由**：码消耗后不能被重复使用，这是安全基本要求。前端检测到"当前无活跃码"时，可在下次进入配对页时自动调用 `generate_pairing_code` 生成新码。

## Risks / Trade-offs

- **并发竞争**：两个设备同时扫同一个码时，`Mutex` 保证只有一个能成功验证（另一个会得到 `InvalidCode` 错误）。这是期望行为，比 DashMap 的 `remove` 语义更明确。→ 无需额外缓解。

- **旧 DHT 记录残留**：切换配对页时快速多次点击"刷新"会产生多个 DHT 记录，最多 300s 后清理。→ 影响极小，对端只有持有当前码才能配对。

## Migration Plan

1. 修改 `PairingManager` 字段和相关方法（后向兼容，命令签名不变）
2. 无数据迁移需求（运行时状态，重启清空）
3. 无配置变更

## Open Questions

- 配对成功消耗码后，是否应由后端自动生成下一个码？
  - 建议：不自动生成，由前端在需要时主动请求，避免不必要的 DHT 写入（用户可能配对完就退出配对页）
