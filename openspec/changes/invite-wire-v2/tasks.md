## 1. 勘察（不阻塞编码，但结论要写回 design.md）

- [x] 1.1 用 `crates/core` 现有的四档夹具核对本 design 的字节账：把 `select_invite_addrs`
      产出的地址逐条 `to_bytes().len()` 打出来，与 design.md「単条地址」表逐行对照。
      **对不上就先改表再动手**——后面所有估算都挂在它上面
  → 逐行对上：tcp 8 / WT 87 / webrtc-direct 48 / circuit 51·53·130，満配地址区 **663**。
    `/p2p/<id>` 后缀实测 +41。design.md 的表已标注「左列实测验证」
- [x] 1.2 Open Question 1（circuit 地址带不带 `/p2p/<self>`）
  → **由代码查实：不带。** `circuit_addr_for` 那条带自身身份的地址唯一消费点是
    `actor.rs:1661` 的 `RelayState::Active`（展示值），不进 `dialable()`。据此**取消**
    `self_id` 省略，`pack`/`unpack` 变纯函数——见 design.md Decision 7
- [ ] 1.3 Open Question 2（未建模形态占比）：真机跑一次 `watch_addrs().get().dialable()`，
      看有多少条会落 `Raw`
  → 需要真机；不阻塞第 2 组，`Raw` 是安全降级

## 2. `crates/net-base` — 紧凑地址编解码

- [x] 2.1 新建 `crates/net-base/src/compact.rs`，定义 `CompactAddrs` / `CompactPath` /
      `Host` / `Wire`，导出 `pack(&[Addr])` 与 `unpack(&CompactAddrs)`（**无 `self_id` 参数**）
- [x] 2.2 实现 `pack`：`Direct` / `Circuit` 的形态识别，certhash 与 relay 两张去重表。
      **完整匹配才走结构化，其余落 `Raw`**（Decision 2）
  → 表里存 multihash / `PeerId` **原字节**而非剥头后的摘要：剥头每项省 6 字节、全表十几字节，
    代价是非 sha2-256 / 非 ed25519 会被静默改写成另一个值。收益来自去重，与剥不剥头无关
- [x] 2.3 实现 `unpack`：按下标还原两张表；单条路径不可还原时**跳过该条**
      并继续（`addr-compact-codec` 的「损坏输入的失败语义」）
- [x] 2.4 护栏测试 · 逐字节往返：四档夹具的全部地址形态 + IPv6 + `/dns4/`，
      断言 `unpack(pack(x)) == x`（比较 `to_bytes()`，不比较 `Display`）
- [x] 2.5 护栏测试 · 未知形态：`/dns4/…` `/quic` `/ws` 与段序异常（TCP + certhash）
      MUST 落 `Raw` 且往返保真
- [x] 2.6 护栏测试 · 去重：5 条地址共用 2 个 certhash → 表长 2；3 条 circuit 同 relay → 表长 1
- [x] 2.7 护栏测试 · 尾部身份段：`…/p2p-circuit/p2p/<id>` MUST 完整保留并逐字节还原
      （防有人把「由调用方补回」那套优化加回来，见 design.md Decision 7）
- [x] 2.8 护栏测试 · 下标越界：手工构造一个引用越界的 `CompactAddrs`，断言只丢那一条、其余还原
- [x] 2.9 体积回归钉 `compaction_halves_the_full_house`：満配那档紧凑后必须不到原地址区的一半
  → 实测 696 → 282 字节（含 3 条 `select_invite_addrs` 不会挑的 quic-v1）；
    按真实选中的 12 条算是 **663 → 约 252**，优于 design 估的 267

## 3. `crates/invite` — wire V2

- [x] 3.1 `InviteWire` 换成 `V2 { core: Vec<u8>, signature: [u8; 64], hints: CompactAddrs }`，
      **删除 V1**（Decision 5）。`PairInvite` 领域类型不变
- [x] 3.2 `encode`：签名对象改为 `b"swarmdrop-invite-v2" ‖ postcard(SignedCore)`，
      **删掉「切末 64 字节」那套位置约定**与 `InviteV1` 上那条「signature 必须末位」的注释契约
- [x] 3.3 `decode_wire`：先验签 `SignedCore`，再 `unpack` 地址提示。结构/版本/验签任一失败
      MUST 整条报错，**不得**产出零地址邀请（Decision 6）
- [x] 3.4 新增等价的裁剪入口——**不需要 secret**，裁完仍验签通过
  → 形态定为 `PairInvite::sign(&secret) -> SignedInvite` + `SignedInvite::{encode, addrs, addrs_mut}`。
    比 `without_addr(index)` 强的一点：`encode` **不吃密钥**，于是「一边裁一边重编码」的循环
    在类型上就不可能顺带重签名——那条性质不再靠注释提醒
- [x] 3.5 护栏测试 · 篡改地址提示后验签仍通过，且 capability/身份/TTL/策略逐字段不变
- [x] 3.6 护栏测试 · 篡改任一受保护字段必须验签失败（逐字段各一条）
- [x] 3.7 护栏测试 · 裁剪后仍验签通过、字段不变，且**全程未用到私钥**（签名函数不可达）
- [x] 3.8 更新 `qr.rs` 的 `qr_size_baseline` 与 `fixed_invite`：wire 变了，287 字符 / 57 模块
      两个基线都要重新校准。**不要随手改数字**——先确认新值符合上表估算再钉

## 4. `crates/core` — 去掉私钥依赖 + 前置闸

- [x] 4.1 `fit_invite_to_scannable` / `drop_least_valuable_addr` 去掉 `secret` 参数，
      循环体从「encode(sign) → QR」变成「encode → QR」
- [x] 4.2 `encode_invite` 相应调整。**三端调用点签名不变**（`encode_invite(&secret, policy)`
      里的 secret 仍用于 `PairInvite::generate`）
- [x] 4.3 更新 `invite_stays_scannable_at_every_scale` 的实测表（doc 里那张四列表格），
      记录新的模块数
- [x] 4.4 **前置闸**：读 4.3 得到的満配模块数判定
  → 実測（上限 98）：家用 **85** / 公网 **89** / CGNAT **89** / 満配 **93** —— 常规设备名下
    **四档全留，裁剪一次都不触发**（wire v1 时満配要从 12 条裁到 8 条）。
  → **但闸门的输入变了**：最坏情况的主导变量不是地址数而是**设备名**。
    `DeviceName::MAX_CHARS = 40`，40 个中文字 120 字节 > 压缩后整个地址区的一半。
    満配 + 顶格设备名不裁是 **101 模块**，超 98 三格，于是仍裁掉 4 条。
  → 落在 design.md 的 **80–98 格**：追加「预算由 UI 传入」后桌面按自己的 240px = 120 模块，
    101 放得下，裁剪归零。**等用户拍板是否扩大范围**（见下方第 7 组）
- [x] 4.5 修正 `INVITE_QR_MAX_MODULES` 文档里那两处已经漂掉的数：桌面码面是 **240px**
      （`generate.lazy.tsx` 传 `size={240}`，组件默认值 260 从未被用过），余量是 120 模块不是 130。
      同步 `src/components/pairing/invite-qr.tsx` 的 props 注释

## 5. 门禁

- [x] 5.1 `cargo fmt --all` · `cargo check --workspace --all-targets` · `cargo test --workspace`
- [x] 5.2 `./scripts/check-wasm.sh` 与 `./scripts/check-wasm.sh --clippy`
      （net-base / invite / core / web 四个受影响 crate 全在 wasm 门禁覆盖内，且 clippy 那条是硬失败）
- [x] 5.3 `./scripts/test-wasm.sh`（`crates/web` 走 `PairInvite::decode`，wire 变了要跑）
- [x] 5.4 桌面 `pnpm exec tsc --noEmit` + `pnpm test`；`docs/` 下 `pnpm typecheck` + `pnpm test`
      → 预期零改动，跑一遍确认 IPC 形态确实没变
- [x] 5.5 `/code-review high` → 修 → 重跑 5.1–5.4
  → 15 条发现，**两条是本改动引入的真缺陷**：
    ① `CompactPath::Circuit` 持 `Box<CompactPath>` ⇒ wire 可无限嵌套，而 postcard 反序列化
      跑在**验签之前** ⇒ 一条上万层嵌套的邀请文本让 `PairInvite::decode` 栈溢出。栈溢出是
      abort 不是 `Err`，而剪贴板是自动读的 ⇒ 发一段文字就能杀掉 App。**递归根本没被用到**
      （circuit 基址只能是直连），改成 `base: Direct` 后由类型保证深度恒为 1。
    ② 地址提示无条数上限。v1 的签名覆盖地址列表，v2 移出后任何能改写邀请文本的人都能塞
      一万条 `/ip4/<受害者>/tcp/<端口>`，受邀方全部登记并拨号 ⇒ 定向连接洪泛
      （`LocalOnly` 也不解决：它过滤成 `is_private_lan()`，那正好是内网扫描清单）。
      加 `MAX_ADDR_HINTS = 32`（生成侧最多 5 类 × 3 传输 = 15，留一倍余量）。
  → 第三条是我自己写的 spec 没落实：解码可能静默产出**零地址邀请**。第一版判据
    「有提示但全丢了才拒」漏掉了最好构造的那种篡改（把路径数直接改成 0），改成
    「零地址一律拒」。三条都补了回归钉，且都过了变异检验（去掉判断变红、加回变绿）。
  → 其余按「改了什么 / 为什么不改」逐条落在下面第 8 组。

## 6. 知识库

- [x] 6.1 `dev-notes/knowledge/net-kernel.md`：记「地址提示为什么不进签名」与
      「紧凑编解码住 net-base 的判据」。这两条都是「下次有人想反过来做」时会踩的
- [x] 6.2 `CLAUDE.md` 的配对段落：wire v2 与「邀请链接长度腰斩」一句话带过；
      **`INVITE_TTL_SECS` / 一次性消费 / canonical 链接三条不动**
  → 顺手修掉一处既有漂移：那段写着「邀请串 `sd:…`，链接走 Base64URL」，而
    `invite-url-canonical` 早已把它换成 https 链接 + base32 payload

## 7. 预算由 UI 传入（闸门判定后追加）

> 闸门落在 80–98 格（満配 + 顶格设备名不裁是 101 模块）。本组把预算从 core 里那个按
> **三端最小码面**一刀切的常量，换成**各端自己的码面 px**。
>
> 关键形态变化：**裁剪从生成侧移到渲染侧**。链接因此保留全部地址（链接根本没有密度上限，
> 现在却在为二维码受委屈），二维码按自己的码面各裁各的，响应式码面天然正确。
> 代价是邀请地址策略必须搬进 `crates/invite` —— 而那本来就是它该在的地方：
> `select_invite_addrs` 与 `drop_least_valuable_addr` 是同一份价值序的正反两面，
> 分居两个 crate 会让它们互相引用的注释变成跨 crate 且无法校验。

- [x] 7.1 `SignedInvite::decode(&str)`：解出 core + signature + 地址，**保留签名**
      （`PairInvite::decode` 丢掉签名，重编码不回去）
- [x] 7.2 新建 `crates/invite/src/compose.rs`，从 `crates/core/src/pairing/manager.rs` 搬入
      `select_invite_addrs` / `append_invite_transports` / `drop_least_valuable_addr`
      与它们的全部测试；新增 `fit_to_scannable(&str, max_modules) -> String`（**无私钥**）
- [x] 7.3 `qr.rs`：`MIN_PX_PER_MODULE = 2`、`max_modules_for(face_px)`；
      `invite_qr_svg` / `invite_qr_matrix` 加 `face_px` 参数并在渲染前 fit。
      删除 core 的 `INVITE_QR_MAX_MODULES`
- [x] 7.4 `crates/core`：`encode_invite` 不再裁剪，返回完整邀请；`shareable_addrs` 改调 invite
- [x] 7.5 三端传自己的**白卡内沿**（不是外框）：桌面 240、移动 220-24=196、Web 196。
      ⚠️ 两端的 `size` 语义本来就不一样（桌面 padding 在外框上、移动端在内），最容易搬错的一步
  → 桌面 bindings 由 `cargo test -p swarmdrop --test specta_export` 重新导出（**不手改**）；
    Web 走 `pnpm build:wasm`；移动端用
    `ubrn generate jsi bindings --library <host dylib>` 重生成 —— 不必跑完整 iOS 构建，
    先 `cargo build -p swarmdrop-mobile-core` 拿宿主 dylib 即可，diff 只有 3 个文件 14 行
- [x] 7.6 护栏测试：同一条邀请在 98 与 120 两个预算下，前者裁后者不裁；
      链接（未裁）地址数 > 二维码（已裁）地址数
- [x] 7.7 重跑第 5 组全部门禁
  → 全绿。⚠️ `mobile` 的 `pnpm lint:ci` 失败，但**在 HEAD 上就已经红**（`notifier.ts` /
    `ports.ts` / `rn-adapter.ts` 等 6 处既有问题），本次改的 `invite-qr.tsx` 单独跑 biome 干净

## 8. code-review 落实（2026-08-13）

- [x] 8.1 wire 去递归 + 非递归护栏钉 `circuit_base_cannot_nest`
- [x] 8.2 `MAX_ADDR_HINTS = 32` + 钉 `too_many_address_hints_are_rejected`
- [x] 8.3 解码拒收零地址邀请 + 钉 `no_tampering_yields_a_zero_address_invite`
  → ⚠️ 这条钉子第一版**没有区分力**：夹具用两条裸 TCP 地址、只翻最低位，去掉被测判断
    照样绿。换成「单条带 certhash 的地址 + 翻遍 8 个 bit 位」后才真正守住
- [x] 8.4 `fit` 从 `compose` 搬进 `qr` 并直接返回 `QrCode`
  → 一次解决三条：渲染路径少一次整码编码（原本每次渲染编两遍）；`qr` ⇄ `compose`
    的互相调用变成单向；解不开的邀请从「原样渲染」改成报错（原先密度预算会静默失效）
- [x] 8.5 `MIN_FACE_PX`（QR version 1 + quiet zone × 2px）：码面小到装不下任何内容时报错，
      不再静默裁到只剩一条地址还交出一张扫不动的码。移动端 `inner` 同时夹到 0
      （负数会在 uniffi 的 u32 转换器里抛一个没有上下文的错）
- [x] 8.6 桌面测试 mock 补上 `facePx` 转发 + 两条断言（此前传没传、传的是码面还是外框，
      测试一律绿）
- [x] 8.7 桌面 `size` 默认值 260 → 240（它现在是地址预算，写大了产出的码跌破 2px/模块）
- [x] 8.8 `intern` 失败回滚：中途失败留下的无引用表项会照常序列化进邀请，白吃字节
- [x] 8.9 测试里那份重复的 `INVITE_QR_MAX_MODULES = 98` 换成传 px
- [x] 8.10 文档：`invite` 模块头还在描述已删除的「签名尾置」契约；`addrs_mut` 指向已搬走的
      守卫；`crates/web` 注释里那个失效的「最坏 327 字节」；CLAUDE.md 的 crate 表两行；
      `lib.rs` 的模块清单；以及「四种用法共用同一个字符串」——二维码现在是裁过的变体

### 遗留（本 change 不做，已记录）

- ~~**本机没有任何可拨地址时生成的邀请是废的，而生成侧不告诉用户。**~~
  **已于 2026-08-13 修复**（`invite-generation-guard`）。原判「会动三端 IPC/FFI 签名」是
  **高估了**：三端的外层签名本来就是 `AppResult<String>` / `FfiResult<String>` /
  `Result<String, JsValue>`，改动只是各加一个 `?`。真正的改动面是
  `PairInvite::generate` 返回 `Result<_, NoDialableAddrs>`（把「地址恒非空」提为
  `PairInvite` 的类型不变量，与 `decode` 那一半咬合）+ `AppError::NoDialableAddrs`
  这个新 kind + 三端文案。
