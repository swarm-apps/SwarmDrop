# pair-invite-ui Specification

## Purpose
TBD - created by archiving change pair-invite-ui. Update Purpose after archive.
## Requirements
### Requirement: 发起方展示可扫描的邀请二维码

发起方屏 SHALL 调 `generate_pair_invite` 取邀请，展示为二维码 + 可复制的 **canonical URL** +
TTL 倒计时。二维码 SHALL 由 `swarmdrop-invite` 用统一规范生成：canonical URL **原样编码**
（不做大小写归一）、最优分段、ECL::M、4 模块 quiet zone、深模块 + 白底不随暗色主题反色、
屏显 ≥260px。

原规范中「payload 大写化走 alphanumeric 模式」的实现方式由「整串 `to_ascii_uppercase`」改为
「payload 本身即大写 base32 + 最优分段把小写前缀单独成段」—— 实测两者码面相同，故不再
牺牲链接外观。

#### Scenario: 生成并展示邀请二维码

- **WHEN** 用户在发起方屏请求生成邀请
- **THEN** 屏上出现该 canonical URL 编码的二维码（白卡包裹）、复制链接按钮、TTL 倒计时

### Requirement: 受邀方经扫码/粘贴/剪贴板消费邀请

受邀方三种输入（移动相机扫码、手动粘贴、剪贴板感知）的前缀校验与解码 SHALL 一律经统一解析
入口，判据为 canonical base URL 而非旧的 `sd:` / `SD` 前缀。

#### Scenario: 移动扫码配对

- **WHEN** 移动用户用应用内扫码器扫描邀请二维码
- **THEN** 统一解析入口验签成功 → 展示对端设备确认卡 → 用户确认 → 配对建立

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

