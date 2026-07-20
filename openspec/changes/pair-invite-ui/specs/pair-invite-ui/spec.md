# pair-invite-ui

## ADDED Requirements

### Requirement: 发起方展示可扫描的邀请二维码

发起方屏 SHALL 调 `generate_pair_invite` 取邀请串，展示为二维码 + 可复制的链接 + TTL 倒计时。
二维码 SHALL 由 core-family（`swarmdrop-invite`）用统一编码规范生成：payload 大写化走 QR
alphanumeric 模式、ECL::M、4 模块 quiet zone、深模块+白底不随暗色主题反色、屏显 ≥260px。

#### Scenario: 生成并展示邀请二维码

- **WHEN** 用户在发起方屏请求生成邀请
- **THEN** 屏上出现该邀请串编码的二维码（白卡包裹）、复制链接按钮、5 分钟倒计时；倒计时归零后可重新生成

### Requirement: 受邀方经扫码/粘贴/剪贴板消费邀请

受邀方 SHALL 支持三种输入：移动相机扫码（`expo-camera` CameraView，qr 过滤 + 前缀校验 +
权限 primer + 粘贴 fallback）、手动粘贴、剪贴板感知一键（桌面 focus 静默读、iOS
`hasStringAsync` 探测亮 chip、Android 读、web 粘贴按钮）。取到邀请串后 SHALL 本地
`PairInvite::decode` 验签，展示对端设备确认卡，用户确认后调 `consume_pair_invite` 发起配对。

#### Scenario: 移动扫码配对

- **WHEN** 移动用户扫描发起方二维码
- **THEN** 本地验签成功 → 展示对端设备名/平台确认卡 → 用户确认 → 配对建立，双方写入配对记录

#### Scenario: 剪贴板感知一键

- **WHEN** 用户复制邀请链接后回到应用（桌面 focus / 移动前台）
- **THEN** 应用感知到剪贴板含 `sdinvite` 前缀内容 → 亮一键配对入口 → 用户点击才真读并 decode → 确认卡

### Requirement: 篡改/过期邀请不进入确认流

受邀方本地 decode SHALL 对篡改（验签失败）、过期的邀请串直接报错，不展示确认卡、不发起配对。

#### Scenario: 过期邀请被拒

- **WHEN** 用户输入一个已过 TTL 的邀请串
- **THEN** 应用提示「邀请已过期」，不进入配对确认

### Requirement: 移动 LAN 近场直连与桌面对称

移动端 SHALL 保留 LAN 近场点按直连（列表点设备 → `pair_direct` → `PairingMethod::Direct`），
与桌面 `directPairing` 对称。invite 用于扫码/跨网，direct 用于同局域网点按。

#### Scenario: 移动近场直连

- **WHEN** 移动用户在设备列表点击一台同局域网设备发起配对
- **THEN** 走 direct 配对（对端 LAN mDNS 校验通过后确认），无需二维码/邀请串
