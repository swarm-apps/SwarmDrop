# pair-invite-link

## ADDED Requirements

### Requirement: 邀请只有一种 canonical 文本形态

配对邀请 SHALL 只有一种对外文本形态：https URL（小写 scheme/host/path + 大写 base32 payload），payload 为 base32-nopad 置于 fragment。

```
https://swarmapp.cn/p/#<BASE32>
```

该字符串 SHALL 同时用作分享链接、二维码内容、剪贴板检测目标与深链移交的 payload 来源。
旧的 `sd:` + base64url 与 `SD` + base32 两种前缀 SHALL 被移除，不保留兼容读取路径。

base URL SHALL 定义为单一常量，切换域名或加境内镜像时只改这一处。

#### Scenario: 发起方复制邀请

- **WHEN** 用户在发起方界面点击复制
- **THEN** 剪贴板中是 canonical URL；粘贴到 IM 或邮件里是一条可点击的链接

#### Scenario: 二维码与链接同源

- **WHEN** 同一份邀请分别渲染为二维码和复制为链接
- **THEN** 两者是**同一个字符串**，不存在两种编码或两个前缀

### Requirement: 系统相机扫码可直接打开落地页

二维码内容 SHALL 是完整的 https URL，使操作系统相机与通用扫码器识别为链接并可直接打开落地页
（不要求已安装 SwarmDrop）。

#### Scenario: 未安装 App 的人扫码

- **WHEN** 一台没装 SwarmDrop 的手机用系统相机扫描邀请二维码
- **THEN** 相机识别出链接并可打开落地页，落地页提供「在 App 中打开」与「在浏览器中配对」两条路

### Requirement: 二维码保持 alphanumeric 密度

二维码编码 SHALL 使用支持 QR 标准最优分段（Annex J）的编码器，使 URL 中的 `#` 不导致整串
降级到 byte 模式。仓库 SHALL 有回归测试钉住「canonical URL 的最优分段版本低于同串强制全
byte 编码的版本」。

#### Scenario: 编码器退化被测试拦住

- **WHEN** 有人把 QR 编码换回全串统一模式的实现，或误关最优分段
- **THEN** 回归测试失败，指出版本号未低于全 byte 基线

### Requirement: 邀请解析在单点收口

从任意文本抽出邀请并验签 SHALL 由 `crates/invite` 的单一函数完成，三端（桌面 / 移动 /
Web）都经各自桥接调用它。SHALL NOT 在 TypeScript、Kotlin 或 Swift 中重写邀请解析器。

#### Scenario: 剪贴板文本中夹带邀请

- **WHEN** 剪贴板内容含 canonical URL（可能带前后空白或被 IM 附加了说明文字）
- **THEN** 统一解析入口抽出邀请并验签，成功则返回邀请内容，失败则返回可区分的错误原因

#### Scenario: 篡改的邀请被拒

- **WHEN** 传入的 URL 中 payload 被修改过
- **THEN** 验签失败，解析返回错误，不进入任何配对流程

### Requirement: capability 不进服务器访问日志

邀请的 capability SHALL 置于 URL fragment，使其不随 HTTP 请求发送到服务器。
落地页 SHALL 从 `location.hash` 读取 payload。

#### Scenario: 打开落地页

- **WHEN** 用户点开邀请链接
- **THEN** 服务器（含 CDN 与边缘日志）只看到路径 `/p/`，看不到 payload

### Requirement: 落地页只做端分流

落地页 SHALL 是纯静态页，不解码、不验签邀请，只提供「在 App 中打开」（跳
`swarmdrop://`，payload 由 JS 从 `location.hash` 显式拼入）与「在浏览器中配对」（同域跳转到
web app 区）两条路径，并可记住用户选择。

落地页 SHALL NOT 尝试探测 App 是否已安装。

#### Scenario: 落地页选择去桌面

- **WHEN** 用户在落地页点「在 App 中打开」
- **THEN** 浏览器尝试打开 `swarmdrop://` 深链，payload 完整传递（不依赖系统自动携带 fragment）

#### Scenario: 落地页选择留在浏览器

- **WHEN** 用户在落地页点「在浏览器中配对」
- **THEN** 同域跳转到 web app 区并携带 payload，由 app 区的运行时单例接管后续配对

#### Scenario: 落地页在慢网络下可用

- **WHEN** 落地页在低带宽环境被打开
- **THEN** 页面不加载 wasm 或其他重资源，仅需极小的静态负载即可完成分流

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
