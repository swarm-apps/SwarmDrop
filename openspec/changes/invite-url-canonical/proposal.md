> # ⛔ 合并阻塞：域名必须先配好
>
> 这个 change 删掉了 `basePath`，所以站点资源路径按**域名根**生成。而 `docs.yml` 的触发
> 分支是 `[main, develop]` —— **一合就部署**。
>
> 2026-07-30 实测的现状：`gh api .../pages` 返回 `build_type: workflow`、**`cname: null`**；
> `dig swarmapp.cn` **零解析记录**。也就是站点仍落在 `https://swarm-apps.github.io/SwarmDrop/`。
> 此时合并 → `_next/*`、`/favicon.ico`、`/app-icon.png` 全部 404 → **整站白屏**，
> 且新发出的邀请链接指向一个不解析的域名。
>
> **`docs/public/CNAME` 解决不了这件事**：本仓是 workflow 型 Pages 部署，GitHub 明确会
> 忽略仓库里的 CNAME 文件（早期版本放过一个，已删）。权威源是 Settings → Pages 字段。
>
> 合并前顺序（详见 `docs/README.md` 的「自定义域名怎么配」）：
> 1. 阿里云配 **apex 的 A/AAAA 记录**（apex 不能用 CNAME 记录）
> 2. Settings → Pages 填 `swarmapp.cn`
> 3. 等 Let's Encrypt 证书签发，勾 Enforce HTTPS
> 4. `curl -I https://swarmapp.cn` 确认 200
> 5. 才合并本 change

## Why

配对流程「不好用」的三个具体表现，都出在邀请的**载体形态**上：

1. **分享出去的东西不可点。** `src/routes/_app/pairing/generate.lazy.tsx:84` 复制的是裸
   `sd:<base64url>` 串。它在微信、邮件、IM 里不是链接，对方只能手动全选复制 → 打开 App →
   找到粘贴框 → 粘贴。每一步都能掉人。
2. **二维码用系统相机扫等于什么都没发生。** 现在的 QR 是 `SD<base32>`，只有本 App 的扫码器
   认。对方没装 App 时扫码是死路 —— 而「对方没装」恰好是最需要引导的时刻。
3. **载体已经分裂成两种编码两个前缀**（链接 `sd:` + base64url、二维码 `SD` + base32），
   剪贴板感知又硬编码了 `startsWith("sd:")`（`use-clipboard-invite.ts:12`）。再加任何新形态
   都要在三端各补一份前缀校验，而这是**信任凭证的解析** —— 漂移就是安全问题。

用户已确认不考虑向后兼容，所以这次直接把载体收敛掉，而不是再加一种。

## What Changes

- **canonical 收敛为单一 URL**：`https://swarmapp.cn/p/#<BASE32>`（小写前缀 + 大写 base32 payload）。
  `sd:` 与 `SD` 两个前缀整体废弃。一个字符串同时是分享链接、二维码内容、剪贴板检测目标、
  深链 payload 来源。系统相机扫码直接打开落地页 —— 这是白拿的新能力。
- **payload 编码 base64url → base32**。payload 要进 QR alphanumeric 字符集的前提（base64url 大小写敏感，大写会毁掉
  payload）。链接长约 20%，但 QR 因为进 alphanumeric 反而更优，且编码从两种变一种。
- **QR 库 `fast_qr` → `qrcode` 0.14**。`fast_qr` 的 `best_encoding()` 是全串统一 mode，
  URL 里的 `#` 会让整串降级 byte mode（+45% 数据量）；`qrcode` 的 `push_optimal_data()`
  做 QR 标准 Annex J 最优分段，`#` 只花约 33 bit ≈ 6 个字符（+3%）。详见 design D3。
- **解析收口**：`crates/invite` 出一个 `parse_invite_text(&str)`，从任意载体文本里抽出
  wire 并验签。三端唯一入口，不在 TS/Kotlin 里重写解析器。
- **域名 swarmapp.cn 整站迁移**：docs 站 + web app + 落地页同域，`PAGES_BASE_PATH` 整个消失。
- **落地页 `/p/`**：纯静态、不 decode、只做分流（「在浏览器中配对」/「复制链接到 App 里粘贴」；
  深链按钮由 `pair-deep-link` 接上，避免中间期出现死按钮）。不做 App 安装探测 ——
  浏览器给不了可靠答案，误判的代价比多一次点击大。

**非目标**：invite 生命周期（TTL / 落盘 / 撤销列表 → `invite-persistence`）；
深链的注册与接收（→ `pair-deep-link`）；剪贴板检测的范围与呈现（→ `pair-deep-link`）；
剪贴板读的原生化（→ `fix-clipboard-native-read`，可独立先合）；ICP 备案（见 design D7）。

## Capabilities

### New Capabilities

- `pair-invite-link`: 配对邀请的单一 canonical 载体 —— 一条 https 链接同时服务链接分享、
  二维码、剪贴板与深链四种场景；capability 置于 fragment 不进服务器日志；解析在
  `crates/invite` 单点收口；落地页只做端分流不解码。

### Modified Capabilities

- `pair-invite-ui`: 发起方复制/展示的内容从裸 `sd:` 串改为 canonical URL；受邀方扫码与粘贴
  的前缀校验改走统一解析入口。

## Impact

- **`crates/invite`**：`invite.rs` 的 `encode`/`decode` 改 base32 + URL 形态、新增
  `parse_invite_text`、`KIND` 常量替换为 base URL 常量；`qr.rs` 换库并重写
  `qr_base32_lowers_qr_version` 测试（换库后该钉的是「最优分段 vs 全 byte 的版本差」——
  这里正是最容易悄悄退化回 byte mode 的地方）；`Cargo.toml` 换依赖。
- **`crates/core` / `crates/web`**：`PairingManager` 与 `WebNode::{connect_invite,
  generate_invite, revoke_invite}` 的调用点跟随；`crates/web/src/node.rs:289/316/332`。
- **桌面 `src/`**：生成页复制 canonical URL；`use-clipboard-invite.ts` 前缀校验改走
  base URL 常量（呈现形态本期不动）。
- **`mobile/`**：扫码前缀校验（`src/app/pairing/scan.tsx`）、生成页复制内容
  （`src/components/pairing/invite-exchange.tsx`）。
- **`docs/`**：新增落地页路由；`next.config` 去掉 `basePath`；`.github/workflows/docs.yml`
  去掉 `PAGES_BASE_PATH` 并加 `CNAME`；README 与文档里的站点链接批量替换。
- **仓库外**：swarmapp.cn 的 DNS 记录（CNAME 到 Pages）+ 仓库 Pages 设置里的自定义域名与
  Enforce HTTPS。这两步是人工操作，写进 tasks 但不在代码里。
- **回归**：`cargo test --workspace`、`./scripts/check-wasm.sh`（invite 在门禁内）、
  三端「生成 → 扫码/点链接/粘贴 → 确认 → 配对」冒烟、`docs` 静态导出后本地起 server 验
  落地页与 app 区路由。

**风险**：

1. ~~**系统相机对大写 scheme URL 的识别率未实测**~~ —— **已消除**。canonical 在实施中改回
   常规小写（`https://swarmapp.cn/p/#…`，见 design D1 的推翻记录），不再需要赌各家扫码器
   是否遵守「scheme/host 大小写无关」这条 RFC 规定。
2. **混合分段 QR 的解码器兼容**：混合模式是 QR 标准的一部分，合规解码器都支持，但值得在
   接线后用真机各扫一次。回退线见 design D4。
3. **未备案 `.cn` 在微信里可能被拦**（design D7）。base URL 是单一常量，被拦后加境内镜像
   或走备案只改一处。
