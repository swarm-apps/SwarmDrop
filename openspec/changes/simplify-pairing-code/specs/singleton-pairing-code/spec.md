## ADDED Requirements

### Requirement: 单例活跃配对码
系统 SHALL 在任意时刻最多维护一个活跃配对码。调用生成接口时，新码立即覆盖旧码（旧码在内存中失效）。

#### Scenario: 生成覆盖旧码
- **WHEN** 系统已存在一个活跃配对码，用户再次调用 `generate_pairing_code`
- **THEN** 新配对码替换旧配对码，旧码立即失效，任何使用旧码的配对请求将被拒绝

#### Scenario: 首次生成
- **WHEN** 当前无活跃配对码，用户调用 `generate_pairing_code`
- **THEN** 系统生成一个 6 位数字码，设置过期时间，发布到 DHT，返回 `PairingCodeInfo`

### Requirement: 配对成功消耗配对码
系统 SHALL 在配对请求被接受后立即消耗当前活跃配对码（置为无效），防止同一码被重复使用。

#### Scenario: 接受配对后码消耗
- **WHEN** 接收方调用 `respond_pairing_request` 并传入 `PairingResponse::Success`
- **THEN** 系统验证码有效后接受配对，当前活跃配对码被清除（置为 None）

#### Scenario: 拒绝配对不消耗码
- **WHEN** 接收方调用 `respond_pairing_request` 并传入 `PairingResponse::Rejected`
- **THEN** 系统拒绝配对，当前活跃配对码保持不变，可继续用于后续配对请求

### Requirement: 配对码过期验证
系统 SHALL 在验证配对码时检查是否已过期，过期码 SHALL 被拒绝并返回 `ExpiredCode` 错误。

#### Scenario: 过期码被拒绝
- **WHEN** 收到配对请求时，内存中的活跃码已超过 `expires_at` 时间
- **THEN** 系统返回 `AppError::ExpiredCode`，配对请求被拒绝

#### Scenario: 有效码通过验证
- **WHEN** 收到配对请求时，内存中的活跃码未过期
- **THEN** 系统允许继续处理配对请求

### Requirement: 无活跃码时拒绝配对
系统 SHALL 在当前无活跃配对码时，拒绝所有使用 `PairingMethod::Code` 的入站配对请求，返回 `InvalidCode` 错误。

#### Scenario: 无码状态下的配对请求被拒绝
- **WHEN** 系统当前 `active_code` 为 None，收到 `PairingMethod::Code` 的配对请求
- **THEN** 系统返回 `AppError::InvalidCode`，不进入配对流程
