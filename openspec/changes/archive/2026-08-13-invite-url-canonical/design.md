# invite-url-canonical 设计

承接 `pair-invite-ui/design.md` D4（「深链本期不做，作后续独立 change」）与
`pair-invite-protocol/design.md` D6 的「后续：深链 / Universal Link（设计文档阶段二的
链接分发）」。本 change 只做**载体形态**，深链的注册与接收在 `pair-deep-link`。

决策依据来自 2026-07-30 的一轮探索，用户逐项确认。

## D1：canonical = 单一 https URL（常规小写外观）

```
https://swarmapp.cn/p/#<BASE32>
```

一个字符串四种用法：分享链接（浏览器可点）、二维码内容（系统相机可扫）、剪贴板检测目标、
深链移交的 payload 来源。`sd:` 与 `SD` 两个前缀整体废弃（用户确认不考虑兼容性）。

**白拿的新能力**：系统相机扫码能直接打开落地页。原来扫 `SD…` 相机什么也做不了，而
「对方还没装 App」正是最需要引导的时刻。这是收敛载体的主要收益，不是副产品。

### 曾经设计成整串大写，实施中被推翻（2026-07-30）

本节原本写的是 `HTTPS://SWARMAPP.CN/P#<BASE32>`，理由是：QR alphanumeric 模式（标准 7.1）
的 45 个字符 `0-9 A-Z space $ % * + - . / :` 不含小写字母，而 URL 的 scheme/host 按 RFC 3986
大小写无关，所以整串大写化能让二维码进 alphanumeric 模式。

**这个理由只在「编码器不支持混合分段」时成立** —— 也就是 D3 换掉的那个 fast_qr。
换成 `qrcode` 之后实测：

| 形态 | 码面 |
|---|---|
| `HTTPS://SWARMAPP.CN/P/#<UPPER>` 全大写 | 57×57 |
| `https://swarmapp.cn/p/#<UPPER>` 小写前缀 + 大写 payload | **57×57（完全相同）** |
| `https://swarmapp.cn/p/#<lower>` 全小写 | 65×65 |

最优分段把 23 字符的小写前缀单独编成一个 byte 段，payload 那两百多字符仍走 alphanumeric，
多出来的 bit 跨不过 version 11 的容量边界。**大写对码面零收益**，却让分享出去的链接看起来
不像正常 URL。

用户在实施中提出这个质疑（「正常来说 url 一般都是小写的吧」），实测后改回常规小写形态。
连带消掉三个负担：

1. `build_qr` 不再需要 `to_ascii_uppercase()`；
2. 两份前缀常量（canonical + 全小写匹配用）合并成一份，`prefix_lower_mirrors_canonical`
   那条同步测试连带删除；
3. **大写 path 的陷阱整个消失** —— macOS 文件系统默认大小写不敏感、Linux 敏感，
   `/P` 本来是个「本地能跑、线上 404」的定时炸弹。

`qr::tests::uppercasing_does_not_shrink_the_code` 钉住这个结论，防止有人凭旧直觉改回去。

payload 仍是**大写** base32：它得落在 alphanumeric 字符集里才有上面那个码面，而随机 token
用大写完全符合常规观感（各种 API key 都是）。

## D2：payload 编码 base64url → base32

payload 要能进 QR alphanumeric 字符集（`A-Z2-7`），而 **base64url 大小写敏感**，
大写化会毁掉它。base32 大小写不敏感，用大写字母表即可，解析侧也能容忍任意大小写的输入
（手抄、IM 自动首字母大写等）。

> 注：D1 原先的「整串大写」在实施中被推翻，但这条结论不变 —— payload 仍需大写落在
> alphanumeric 里，只有 scheme/host/path 前缀回到了小写。

| | base64url | base32 |
|---|---|---|
| 每字符信息量 | 6 bit | 5 bit |
| 链接长度 | 基准 | +20% |
| QR 模式 | byte（大小写敏感，不能大写） | alphanumeric（可大写） |
| 全仓编码种类 | 两种并存 | 一种 |

交换划算：链接长度用户不在意（是点的，不是抄的），QR 密度直接决定扫码成功率。
副产品是 `data-encoding` 的两个 alphabet 用法收敛成一个。

## D3：QR 库 fast_qr → qrcode 0.14

**问题**：`#` 不在 QR alphanumeric 字符集内（`?`、`=` 同样不在）。而 `fast_qr` 0.13.1 的
模式选择是**全串统一**，读源码 `src/encode.rs`：

```rust
fn try_encode_alphanumeric(input: &[u8], i: usize) -> Mode {
    for &c in input.iter().skip(i) {
        if !is_qr_alphanumeric(c) {
            return Mode::Byte;      // 一个字符不合格 → 整串降级
        }
    }
    Mode::Alphanumeric
}
```

不做混合分段。于是在 fast_qr 下，「单一 URL + capability 放 fragment + QR 保持 alphanumeric」
三者不可能同时成立 —— 这才是 D1 一度要求整串大写的根源。换掉编码器后三者同时成立，
连大写都不必要了。

**解法**：`qrcode` 0.14.1 提供 `push_optimal_data()`（QR 标准 Annex J 最优分段）与
`push_segments()`。混合模式是标准的一部分，合规解码器都支持。`#` 触发两次 mode 切换，
开销约 33 bit ≈ 6 个 alphanumeric 字符：

| 方案 | 分段 | 总 bit（payload 按 base32 200 字符估） |
|---|---|---|
| payload 放 path（无 `#`），全 alnum | 单段 alnum | ~1236（基准） |
| **fragment + 最优分段** | alnum / byte / alnum | **~1274（+3%）** |
| fragment + fast_qr | 单段 byte | ~1796（+45%） |

依赖形态：`qrcode = { version = "0.14", default-features = false, features = ["svg"] }`。
`svg` feature 不拉任何额外依赖；默认 features 里的 `image` 会拉 `image ^0.25`，**必须关掉**
（体积 + wasm 门禁）。纯 Rust，无系统依赖。

**顺带记一个库的 bug**：`fast_qr` 的文档注释把 alphanumeric 字符集写成 `$%*./:+-?.=`，
列了 `?` 和 `=`，但实现里 `ascii_to_alphanumeric` 碰到它们直接 `panic!`。注释是错的，
以后谁再评估这个库别信那行。

**回归钉子**：现有测试 `qr_base32_lowers_qr_version` 钉的是「base32 比 base64url URL 版本低」，
换库后语义失效。要重写成「canonical URL 的最优分段版本 < 同串强制全 byte 的版本」——
悄悄退化回 byte mode 是这块最可能发生的回归，必须有测试守。

## D4：capability 放 fragment，并留一条回退线

`#` 后的内容浏览器不发给服务器 —— GitHub Pages 边缘日志、Referer、CDN 记录都拿不到
capability。放 query 或 path 则会进访问日志与浏览器历史同步。capability 是 128bit 信任凭证，
这条边界不该用「风险很低」来交换。`pair-invite-protocol/design.md` D7 里写的
`https://swarmdrop.app/i#` 已经是 fragment，方向一致。

**代价**：fragment 在跳深链时容易丢（部分 Android intent 转发、IM 内置浏览器 URL 重写会吃掉
`#` 之后）。所以 https → 深链的移交必须在 JS 里显式读 `location.hash` 拼进
`swarmdrop://`，不能指望系统自动带过去。这条约束落在 `pair-deep-link` 的落地页按钮实现上。

**回退线（用户决定不做前置 spike，故预案写在这里）**：如果实测发现混合分段 QR 有解码器
兼容问题，回退动作是
**payload 从 fragment 挪到 path 段**（`https://swarmapp.cn/p/<BASE32>`，`/` 在 alphanumeric
字符集内）：

- 单一载体与 alphanumeric payload 全部保住，**不必换回 QR 库**（D3 的换库对两种形态都有益）
- 代价是 capability 进服务器访问日志 —— 由 24h TTL + 一次性消费 + 验签 + 可撤销
  （`invite-persistence`）共同兜底，风险从「不可能」降到「低」
- 改动面：`crates/invite` 的 URL 拼装与解析各一处，落地页读 `location.pathname` 而非 `hash`

## D5：落地页只分流，不 decode

落地页 `/p/` 是纯静态页，**不解码不验签**，只做一件事：把 fragment 原样交给桌面 App
（`swarmdrop://`）或 web app 区（同域 `next/link` 跳 `/app/devices#…`），两个按钮 + 记住选择。

理由：

- **体积**。要 decode 就得为 `crates/invite` 单独编一个薄 wasm 包（Rust wasm 最小也几十 KB），
  而这个页面是分享链接的第一跳，国内经 GitHub Pages 的可达性本来就不确定（D7）。
  不 decode 能压到 10KB 以内，慢网络也能开。
- **安全闸没有损失**。身份确认卡（设备名 / 平台 / 短指纹）在 App 侧与 web app 区本来就有，
  那里有完整的 decode 与验签能力。落地页多做一次预览不增加任何安全性。
- 顺带消掉一个 spike：「要不要为落地页单独出薄 wasm 包」这个问题不存在了。

**代价**：落地页上只能写「有人邀请你配对」，写不出对方设备名。可接受 —— 用户到 App 侧
一步之后就能看到完整身份，且那一步本来就必须走。

**不做 App 安装探测**。跳 scheme 后用定时器测失焦的那套 hack 各浏览器行为不一，没装 App 时
还会留个报错页。并列两个按钮诚实、零误判，也避免「因为能探测就想省掉确认步骤」的滑坡。

## D6：swarmapp.cn 整站迁移，而非只给落地页

docs 站 + web app + 落地页同域。收益：

- `PAGES_BASE_PATH` / `next.config` 的 `basePath` **整个消失**，静态导出少一层坑
  （`web-app-frontend.md` 里记的 basePath 与 `next/link` 那条约束自然解除）
- 落地页跳 web app 区是**同域 `next/link`**，不是跨域跳转
- canonical 更短 → QR 更疏

若只给落地页用（备选，未采纳）：canonical 能短到 `https://swarmapp.cn/#PAYLOAD`（连 `/p`
都省），但「在浏览器中配对」要跨域跳回 `swarm-apps.github.io/SwarmDrop/app/…`，体验割裂，
basePath 也还在。

**迁移不会断链**：Pages 配置自定义域名后，原 `swarm-apps.github.io/SwarmDrop/` 会 301 到新域名。

## D7：不做 ICP 备案（用户决策）

域名 `swarmapp.cn` 在阿里云，实名认证已完成，状态正常。CNAME 到 GitHub Pages ——
服务器在境外，**不在工信部备案管辖范围内**，解析与访问正常。备案（ICP）与域名实名认证是
两件事，后者是解析的前提且已满足。

未备案的两个实际风险：

1. `.cn` 受工信部直管，未备案 + 境外服务器的政策风险高于 gTLD；被投诉或抽查时注册局有能力
   直接 serverHold。
2. **微信拦截概率更高**，而微信很可能是配对链接的主要分享渠道。

不备案的理由：备案要境内云资源 + 2~3 周审核，会直接堵住这个 change；而微信拦不拦是概率
事件，必须实测才知道。**缓解**：base URL 抽成单一常量（本来就要做），被拦后加境内镜像或
补备案只改一处。国内 CDN 一律要求备案才能接入，这条在未备案期间也用不上。

## D8：不做前置 spike（用户决策）

原本列了三件待实测：相机对**大写 scheme** 的识别率、混合分段 QR 的真机解码、macOS 深链与
share-target 的事件分流（后者属 `pair-deep-link`）。用户判断风险可接受，不做前置 spike。

**第一项已随 D1 改回小写而消失**，这是那次修正的最大收益：原方案要赌各家扫码器都遵守
「scheme 与 host 大小写无关」这条 RFC 规定，而 `https://swarmapp.cn/p/#…` 是完全标准的 URL
形态，所有扫码器都认。整个方案里唯一「没有先例、只能赌」的一环就此消失。

剩下两项：混合分段 QR 的真机解码（混合模式是 QR 标准的一部分，合规解码器都支持，
但仍应真机各扫一次作为验收，见 tasks Phase 5）；事件分流属另一个 change。

D4 的回退线仍然保留，作为「万一某类解码器不支持混合分段」的预案。
