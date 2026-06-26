## Why

当前配对码实现使用 `DashMap<String, PairingCodeInfo>` 支持多个同时活跃的配对码，但实际使用场景中同一时刻只需要展示一个配对码给用户扫描/输入。多码并行带来了不必要的复杂度：无自动清理机制、前端展示逻辑模糊、DHT 发布重复。用单例模式替换后逻辑更清晰，符合 LocalSend 式"一码等待配对"的 UX 心智模型。

## What Changes

- 将 `PairingManager.active_codes: DashMap<String, PairingCodeInfo>` 替换为 `active_code: Arc<Mutex<Option<PairingCodeInfo>>>`
- `generate_code()` 覆盖已有码（自动作废旧 DHT 记录），只保留一个活跃码
- 配对成功后自动消耗当前码（置为 `None`）
- 配对码过期后前端轮询 `is_expired()` 可自动调用 `generate_code()` 刷新
- `handle_pairing_request` 中验证逻辑改为读取 `active_code` 而非从 map 中按 key 查找

## Capabilities

### New Capabilities

- `singleton-pairing-code`: 单例配对码管理 —— 同一时刻只有一个活跃配对码，生成新码覆盖旧码，配对成功后自动消耗

### Modified Capabilities

（无现有 spec，本次为首次引入规范）

## Impact

- `src-tauri/src/pairing/manager.rs` — 核心改动，`active_codes` 字段替换
- `src-tauri/src/pairing/code.rs` — 数据结构无需改动
- `src-tauri/src/commands/pairing.rs` — `generate_pairing_code` 命令行为不变，前端 API 兼容
- 前端 `src/stores/pairing-store.ts` / 配对页面组件 — 逻辑不变（仍调用 `generate_pairing_code` 获取码）
