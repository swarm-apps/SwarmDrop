# invite-url-canonical 任务分解

## Phase 0 — 编码层收敛（crates/invite）

- [x] base URL 常量 `INVITE_URL_PREFIX = "https://swarmapp.cn/p/#"`（单一事实源）替换
      `KIND = "sd"`。**全小写字面量** —— `extract_payload` 拿它去 `find` 一个已小写化的串，
      前缀里混进大写会让所有邀请都解析不出来（大写 path 的初版正是这么踩的），
      由 `prefix_is_all_lowercase` 钉住
- [x] `PairInvite::encode` 改为 base32-nopad + canonical URL 拼装
- [x] `PairInvite::qr_payload` **删除** —— base32 之后二维码不再需要另一种编码，
      它就是 canonical 串原样
- [x] `decode` 改造成「从任意文本提取」：`extract_payload` 定位前缀（大小写不敏感）+
      吃掉紧随的 base32 字符。**没有另起 `parse_invite_text`** —— `decode` 本来就是那个
      函数，加一个同义 API 只会多一个入口（见下方「对 artifacts 的偏离」）
- [x] `decode_wire_text` 删除（职责并入 `decode` + `extract_payload`）
- [x] 单测：canonical 往返、前缀全小写、整串大写形态仍可解析（输入健壮性）、
      脏文本提取（6 种包裹形态）、逐字节篡改全拒、
      旧 `sd:` 形态被拒、别的域名被拒、无 payload 被拒、非邀请文本被拒
- [x] 全仓无旧形态残留（`rg 'qr_payload|decode_wire_text'` 归零）

## Phase 1 — QR 换库（crates/invite）

- [x] `Cargo.toml`：移除 `fast_qr`，加
      `qrcode = { version = "0.14", default-features = false, features = ["svg"] }`
      （`cargo tree` 确认 `image` 未被拉入）
- [x] `qr.rs` 改用 `qrcode`：`QrCode::new` 内部即 `bits::encode_auto`（最优分段）
      且默认 EC level 就是 M —— 正好是原规范，无需显式设置
- [x] `invite_qr_svg` / `invite_qr_matrix` 签名不变，三端渲染组件零改动
- [x] 重写回归钉子 `optimal_segmentation_beats_byte_mode`：钉「canonical（byte+alnum 两段）
      version < 同串全小写（单段 byte）」；另加 `uppercasing_does_not_shrink_the_code`
      钉住「大写对码面零收益」，防止有人凭旧直觉把 canonical 改回全大写
- [x] 新增 `qr_size_baseline`：实测 268 字符 → 57×57（**version 10**，`width = 4v+17`）；
      同串 byte mode 65×65（**version 12**，面积 +30%）。
      ⚠️ 初版把版本号写成 11/13 —— 由 review 抓出并修正；那条注释自己写着「不要随手改数字」，
      却把校准要用的数字写错了
- [x] 新增 `build_qr_encodes_input_verbatim`：用**模块矩阵**（而非宽度）钉住「不做大小写
      归一」。review 指出 `qr_size_baseline` 的 `width == 57` 对「把大写化加回 build_qr」
      零区分力（大写与原样码面相同，加回去照样绿），而后果是**扫码跳网页 404**
      （落地页只有小写 `/p/`，Pages 区分大小写）
- [x] `optimal_segmentation_beats_byte_mode` 改为经 `build_qr` —— 原来直接调 `QrCode::new`
      绕过被测函数，实质只是「依赖库行为」的金丝雀
- [x] `cargo test -p swarmdrop-invite` 22/22 + `./scripts/check-wasm.sh` 绿

## Phase 2 — 域名与站点迁移

> ⛔ **前三条是合并阻塞项**，见 proposal 顶部的警告：现状是 `cname: null` + 域名零解析，
> 此时合并会整站白屏。

- [~] **延后（待备案）**：阿里云配 **apex 的 A/AAAA 记录**（apex 不能用 CNAME 记录）
- [~] **延后（待备案）**：仓库 Settings → Pages 填 `swarmapp.cn`（**权威源；仓库里的 CNAME 文件
      在 workflow 型部署下会被忽略**）
- [~] **延后（待备案）**：等 Let's Encrypt 证书签发 → 勾 Enforce HTTPS → `curl -I https://swarmapp.cn`
- [x] ~~`docs/public/CNAME`~~ **已删除** —— 本仓 workflow 型部署会忽略它，留着只会让下一个人
      以为域名配好了（实测 `gh api` 的 `cname` 仍是 null）
- [x] `docs/next.config.mjs`：删 `basePath`，并**整层删掉 `env:` 注入** ——
      `site.ts` 只被构建期服务端消费（metadataBase / sitemap），直接读 `process.env`
      即可，不必转成 `NEXT_PUBLIC_*`
- [x] `docs/lib/site.ts`：不再导出 `BASE_PATH`；**CI 下缺 `PAGES_SITE_ORIGIN` 直接 throw**
      —— review 指出这个失效原本是安静的（构建成功、产出 localhost sitemap、线上零症状），
      而迁移前 basePath 一空是「响的」。已实测：`CI=1 pnpm build` 会失败
- [x] `docs/lib/shared.ts`：`appIconPath` 简化为 `"/app-icon.png"`
- [x] `docs/app/layout.tsx`：favicon 路径去掉前缀拼接
- [x] `.github/workflows/docs.yml`：`PAGES_BASE_PATH` → `PAGES_SITE_ORIGIN: https://swarmapp.cn`
- [x] 全仓站点链接替换（README ×2、mobile/README ×2、docs/README、桌面关于页），
      `rg 'swarm-apps\.github\.io'` 在代码与 README 中归零
- [x] 顺带修两处早已失真的 README 说法：`sdinvite…`（更早的形态）、capability「256-bit」
      （实际 `[u8; 16]` = 128-bit）、Web 端入口 `/try`（实际是 `/app`）
- [x] 本地产物验证：`/app/devices/` 返回 200，导航 href 是裸路径无前缀
- [~] **延后（待备案）**：部署后验证新域名可访问、旧 github.io 301、`_next/*` 不 404

## Phase 3 — 落地页 /p/

- [x] **改为 `docs/public/p/index.html` 手写纯 HTML，不走 Next 页面**（见下方偏离说明）
- [x] 内容：有 payload → 「在浏览器中配对」+「复制邀请链接」+ App 引导文案；
      无 payload → 「这个链接不完整」+ 出口
- [x] 明暗配色（CSS 变量 + `prefers-color-scheme`）、响应式、`noindex`
- [x] payload 校验 `^[A-Za-z2-7]+$`，**不进 DOM**（只用于跳转与 storage），零 XSS 面
- [x] 递交改用 `sessionStorage`（key `swarmdrop:pending-invite`，值为完整 canonical 链接），
      storage 不可用时退回 fragment
- [x] 体积实测 **4.5KB gzip**（修完 review 的三条后，原 3.2KB），零 JS 框架、零额外请求
- [x] 本地验证 `/p/` 命中落地页
- [x] **review 修复**：递交改用重建的 canonical（`location.origin + "/p/#" + payload`）而非
      `location.href` 原文 —— 原文会带微信追加的 `?from=singlemessage`，夹在中间就让后端
      `extract_payload` 匹配不上（症状：落地页显示成功、app 区报「不是配对邀请链接」）
- [x] **review 修复**：`sessionStorage.setItem` 从页面加载时移到**点击时** —— 用户不点就离开
      的话，那条 key 会在 tab 里留一整天（TTL 24h），之后进 app 区莫名冒出陌生邀请预填
- [x] **review 修复**：`navigator.clipboard` 在非安全上下文 / in-app WebView 里是 undefined，
      访问 `.writeText` **同步抛 TypeError**（不是 rejected promise）→ 按钮点了毫无反应。
      已加 try/catch + 独立的 `role="status" aria-live` 状态行 + 2.5s 自动恢复
- [x] **review 修复**：`#bad` 分支改为**默认可见**（渐进增强反过来）—— 原来两个分支都
      `hidden` 起步，JS 被禁时整页只剩一个色块
- [x] **review 修复**：出口链接补尾斜杠（`/app/devices/`、`/docs/`）—— 站点是
      `trailingSlash: true`，少了要吃一次 301，而兜底路径的 payload 挂在 fragment 上，
      正是 design D4 自己警告「跳转中容易丢 fragment」的场景

## Phase 4 — 三端适配

- [x] web app 区 `pairing-panel.tsx`：handoff effect 消费 sessionStorage / fragment，
      读完即清（`removeItem` + `history.replaceState`），预填输入框但**不自动发起**
- [x] **review 修复**：`replaceState` 改为把 `history.state` 原样传回，且只在 fragment
      兜底路径执行。传 `null` 会抹掉 Next app-router 在 `useInsertionEffect` 里写入的内部
      字段（本 effect 是 passive、子先父后，跑在 router 给 `replaceState` 打补丁之前），
      `onPopState` 的 `if (!event.state) return` 随即让该 history entry 失活 ——
      表现为按浏览器后退键地址栏变了、页面不动。主路径地址栏本来是干净的，清它零收益
- [x] web placeholder 改 canonical
- [x] `crates/web/src/node.rs` **无需改动** —— `connect_invite` / `generate_invite` 只是转发给
      core，文本形态变化对它透明
- [x] 桌面 `use-clipboard-invite.ts`：前缀校验改 canonical 正则，且从 `startsWith` 改成
      **在文本中搜索**（与后端脏文本提取行为对齐）
- [x] 桌面 placeholder / store 注释 / 二维码组件注释
- [x] 移动 `scan.tsx`：扫码用锚定正则、粘贴改非锚定（微信复制常带说明文字），
      共享一份 `INVITE_LINK_SOURCE`；连带放宽那条「不要做大小写归一」的注释
- [x] 移动 `invite-exchange.tsx` placeholder（其剪贴板路径本就不做前缀校验、直接交后端，无需改）
- [x] `crates/core/src/pairing/manager.rs` **无需改动**（同 crates/web 的理由）
- [x] i18n **无需 extract** —— 改动全在代码注释与 placeholder 字面量，`.po` 里的 UI 串未变

## Phase 5 — 门禁与验收

- [x] `cargo fmt --all` / `cargo check --workspace --all-targets` / `cargo test --workspace` 全绿
- [x] `./scripts/check-wasm.sh` + `--clippy` 绿；`cargo clippy -p swarmdrop-invite` 无 warning
- [x] `pnpm exec tsc --noEmit`（桌面）、`pnpm test` 64 passed、`pnpm check:clipboard`
- [x] `mobile` 下 `pnpm typecheck`、`docs` 下 `pnpm build`
- [x] 知识库：`web-app-frontend.md` 作废 basePath 两条老约束 + 新增「极小页面不要走 Next」
      与「跨页面递交 capability 用 sessionStorage」两节
- [ ] **人工**：真机扫码（iPhone 系统相机 + Android）确认二维码被识别为链接并能打开落地页（canonical 已是标准小写 URL，风险已大幅下降）
- [~] **延后**：微信内打开落地页，验证未备案 `.cn` 是否被拦（design D7）——
      现在载体是 github.io，这条要改成验证 github.io 在境内的可达性
- [ ] **人工**：三端配对冒烟（含 `fix-clipboard-native-read` 延后的那条剪贴板冒烟）

## 实施中对 artifacts 的偏离

1. **没有新增 `parse_invite_text`**。tasks 原写「新增 `parse_invite_text`」，但 `PairInvite::decode`
   本来就是「从文本解析 + 验签」这件事，只是文本层需要改造。加一个同义函数等于多一个入口，
   与「解析在单点收口」的目标相反。已改造 `decode` 本身，`spec.md` 的措辞是「单一函数」，不受影响。
2. **落地页从 Next 页面改为 `public/` 下的手写 HTML**。design D5 定的目标是「<10KB」，
   而 Next client component 的 baseline 就 ~150KB gzip（React + framework runtime），
   在 Next 里做不到。改为 `docs/public/p/index.html` 后实测 3.2KB。
   连带：`app/p/page.tsx` 不存在，故「`useSearchParams()` 要 Suspense」「路由段大小写」这两条
   Next 侧注意事项不适用（但大小写敏感的部署风险仍在，见 Phase 3 与知识库）。
3. **递交方式从 fragment 改为 sessionStorage 优先**。原设计是落地页跳
   `/app/devices#<payload>`；实施时发现同域下 sessionStorage 更好：capability 不进第二个
   地址栏，且存完整 canonical 链接让 app 区不必再硬编码一份前缀。fragment 保留为
   storage 被禁用时的兜底。
4. **canonical 带尾斜杠 `/p/#` 而非 `/p#`**。静态导出产物是 `p/index.html`，`/p` 会先吃一次
   目录重定向；带尾斜杠省掉那一跳，也避开重定向丢 fragment 的风险。
5. **QR 的 ECL 无需显式设置**：`qrcode::QrCode::new` 默认就是 `EcLevel::M`，与原规范一致。

## 收尾核对带出的待办（2026-07-30）

- [ ] **前缀在 JS 侧有两份功能性镜像，不经 FFI**：`mobile/src/app/pairing/scan.tsx` 的两个
      正则、`docs/app/app/_components/pairing-panel.tsx` 的 fragment 兜底。改域名漏掉任一处
      的表现是**静默失败**（扫码没反应 / 拼出的串被后端拒），没有报错。
      根治要把 `INVITE_URL_PREFIX` 从 wasm 与 uniffi 导出去给 JS 用；当前先在常量的文档
      注释里列了镜像清单。
      注意 Web 兜底那处**必须硬编码域名**，不能改成 `location.origin` —— 解码器只认
      canonical 前缀，Pages 子路径/预览域名下 origin 拼出的串会被拒。

## 载体改为 GitHub Pages 子路径（2026-07-30，用户决策）

`swarmapp.cn` 已实名但**未备案**，境内注册商可随时停掉未备案 `.cn` 的解析。把它当邀请链接的
载体等于把配对功能挂在一个可被第三方关停的开关上，所以自定义域名整体延后。

- [x] 主前缀改为 `https://swarm-apps.github.io/SwarmDrop/p/#`
- [x] **新增 `ACCEPTED_URL_PREFIXES`：解码受理列表，生成只用主前缀。** 迁移期两种链接会同时
      在外面飘（邀请活 24h，用户手上的版本也不会同一秒更新），只认主前缀 = 迁移当天在途邀请
      一起失效，症状还是最难自查的「链接看着好好的，点开说不是配对邀请」。
      受理多个前缀不放松安全性 —— 前缀从来不是信任边界，签名才是（`evil.example` 那条负例仍拒）
- [x] docs 恢复 `basePath: /SwarmDrop`（`next.config.mjs` / `site.ts` 的 `BASE_PATH` /
      `shared.ts` 的 `appIconPath` / `docs.yml` 两个 env），实测产物 `_next/*`、sitemap、
      metadata icon 均带前缀
- [x] **落地页的根路径写法全改相对路径**（`../app/devices/`）。它是 `public/` 下的纯 HTML、
      不经 Next 处理，拿不到 basePath 自动前缀 —— 原来的 `/app/devices/` 在子路径下全 404。
      相对路径在「子路径」与「将来的域名根」两种部署下都对，且不依赖 JS
- [x] 落地页的 canonical 重建从 `location.pathname` 推 base，不再假设站点在域名根。
      实测四种情形：`/SwarmDrop/p/`、`…/p/index.html`、域名根 `/p/` 都拼出受理前缀；
      localhost 拼不出（既有限制，已写进注释）
- [x] 移动端正则与 Web 兜底改为**两个前缀都受理** → 将来切主域名**不必再改这两处**
- [x] 用户可见的官网链接（README ×2、桌面关于页、移动关于页）改指 Pages，
      不再把用户指向一个打不开的域名
- [x] `docs/README.md` 重写：把「怎么配域名」改成「待备案 + 备案后四处一起改」的对照表

### 顺带修掉一条结构性 flaky 测试

`qr::tests::qr_size_baseline` 用 `sample_invite()`（每次随机生成密钥与 capability）断言精确
宽度。前缀变长后码面正好卡在 version 边界上，这条测试开始**随机红绿** —— 实测同一份 wire 的
随机样本落在 57~65 之间（纯字母 57、纯数字 49、周期混排 65、真实随机多数 61），因为 base32
里的 `2-7` 属 QR 的 numeric 类（3.33 bit/字符），数字连段长短会改变最优分段的取舍。

- [x] 改用 `fixed_invite()`：固定私钥（protobuf 字节）+ 固定 capability。ed25519 签名是
      确定性的，所以整串字节可复现 ⇒ 宽度可复现。连跑 20 次绿
- [x] 注释里写明「这是哨兵，不是生产码面就是 57 的承诺」
- [x] `uppercasing_does_not_shrink_the_code` → `uppercasing_never_grows_the_code`：
      原来的结论（「大写对码面零收益」）在长前缀下**不再成立**，实测某些 payload 下大写能省
      一个 version。但仍不能大写 —— `/SwarmDrop/` 是仓库名、**Pages 路径区分大小写**，
      大写成 `/SWARMDROP/` 直接 404。所以测试只钉「大写不会更大」这一个方向，真正防
      「把 `to_ascii_uppercase()` 加回 `build_qr`」的是 `build_qr_encodes_input_verbatim`

### 一个被新前缀立刻暴露的解码 bug

- [x] **`extract_payload` 要把前缀也小写化再比。** 它在小写化后的 haystack 上 `find`，而
      Pages 路径段是仓库名 `SwarmDrop`（带大写），直接拿常量去比**永远匹配不上** ——
      5 条既有测试同时红，说明解码对新前缀整体是坏的。生成侧必须保留精确大小写
      （Pages 路径区分大小写），所以「生成精确、匹配不敏感」这个不对称是刻意的。
      `prefix_is_all_lowercase` 那条不变量随之被推翻，改成 `prefix_matching_is_case_insensitive`
