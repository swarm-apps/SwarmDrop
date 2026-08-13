## MODIFIED Requirements

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
