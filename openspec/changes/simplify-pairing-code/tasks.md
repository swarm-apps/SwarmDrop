## 1. 重构 PairingManager 字段

- [x] 1.1 将 `active_codes: DashMap<String, PairingCodeInfo>` 替换为 `active_code: Mutex<Option<PairingCodeInfo>>`
- [x] 1.2 更新 `PairingManager::new()` 初始化逻辑，移除 `DashMap` 初始化，添加 `Mutex::new(None)`

## 2. 更新 generate_code 方法

- [x] 2.1 将 `generate_code` 内的 `active_codes.insert(...)` 替换为锁定 `active_code` 并写入新码
- [x] 2.2 验证新码覆盖旧码的语义正确（旧码直接被替换，无需显式删除 DHT 记录）

## 3. 更新 handle_pairing_request 验证逻辑

- [x] 3.1 将 `active_codes.remove(code.as_str())` 替换为：锁定 `active_code`，验证码字符串匹配，然后取出（置 None）
- [x] 3.2 验证：无活跃码时返回 `AppError::InvalidCode`
- [x] 3.3 验证：码已过期时返回 `AppError::ExpiredCode`
- [x] 3.4 验证：拒绝配对时（`PairingResponse::Rejected`）不消耗码，将码放回

## 4. 清理不再需要的代码

- [x] 4.1 移除所有对 `active_codes` 的引用（确认无遗漏）
- [x] 4.2 检查是否有外部代码直接访问 `active_codes` 字段（如有则一并更新）

## 5. 测试验证

- [ ] 5.1 运行 `cargo build` 确认编译通过
- [ ] 5.2 运行 `cargo clippy` 确认无警告
- [ ] 5.3 手动测试：进入配对页 → 生成码 → 再次点击刷新 → 确认只有新码有效
- [ ] 5.4 手动测试：配对成功后 → 确认旧码不可再用（重新进入配对页应生成新码）
