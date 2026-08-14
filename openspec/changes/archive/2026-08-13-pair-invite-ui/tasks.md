# pair-invite-ui 任务分解

## Phase 0 — invite 下沉 crates/invite + QR 生成（D1/D2/D7）

- [x] 新建 `crates/invite`（swarmdrop-invite），workspace 登记；`pairing/invite.rs` 迁入（PairInvite/InviteRegistry/wire + 单测），依赖 net-base + sha2/postcard/data-encoding
- [x] QR 模块：`fast_qr` 生成 `pair_invite_qr_svg(&str) -> String`（内部 `.to_ascii_uppercase()` + ECL::M + 4 模块 quiet zone + 深/白配色）；或 `qr_matrix(&str) -> Vec<Vec<bool>>` 供三端自绘
- [x] core `pairing/manager.rs` 改从 `swarmdrop-invite` 引入；删 `crates/core/src/pairing/invite.rs`
- [x] `crates/invite` 进 `scripts/check-wasm.sh`（第七 crate），双 target 绿；`cargo test -p swarmdrop-invite`
- [x] QR 大写→版本降档单测（v13-15 → v11-12）；`cargo test --workspace` 全绿

## Phase 1 — 桌面 React UI

- [x] **前置修复**：`pairing-store.ts` 去掉 `PairingCodeInfo`/`generatePairingCode`/`getDeviceInfo`/`searchDevice`/`found` 态，`pnpm exec tsc --noEmit` 恢复通过
- [x] QR 渲染组件（消费 core 出的 SVG/矩阵，白卡包裹，≥280px）
- [x] 发起方屏：`generatePairInvite(localOnly)` → 二维码 + 复制链接 + 5min 倒计时 + LocalOnly 开关 + 重新生成
- [x] 受邀方屏：粘贴框 + 剪贴板感知（focus 静默读 + `sdinvite` 前缀 → 顶部一键条）→ `consumePairInvite`
- [x] 删 `generate.lazy.tsx`/`input.lazy.tsx` 6 位码屏、`add-device-section` 码面板 + `PairingInputDialog`
- [x] 复用校验：`connection-request-dialog`/`device-icon`/入站 store 分支/`directPairing`/`use-pairing-success`/`task-surface`
- [x] i18n：废码文案 + 新增邀请文案，`pnpm i18n:extract`

## Phase 2 — 移动 RN UI

- [x] mobile rust 补 `pair_direct(peer_id)`（D6）；uniffi 绑定重建（ubrn generate jsi bindings，从 dylib）
- [x] `expo install expo-camera`（→ `~56.0.8`）+ `app.json` config plugin（cameraPermission 文案，禁麦克风/RECORD_AUDIO）；`expo prebuild` 重编为构建期动作
- [x] QR 渲染组件（`react-native-svg` 按矩阵画 `<Rect>`）
- [x] 发起方卡/屏：`generatePairInvite` → 二维码 + 复制（`expo-clipboard`）+ 倒计时
- [x] 受邀方：`CameraView` 扫码（qr 过滤 + 去抖 + 前缀校验 + 权限 primer + `openSettings` fallback）+ 粘贴 + 剪贴板感知（`hasStringAsync` 探测亮 chip）→ `consumePairInvite`（`mobile/src/app/pairing/scan.tsx`；连带修 `fix(invite)` 前缀大小写）
- [x] 删 `pairing-code-store`、`PairingCodeCard/Sheet/Input`、`lookupDeviceByCode` 链；`respondPairingRequest` 去 code 参数；LAN 近场 `handlePair` 接 `pairDirect`
- [x] 复用：`peer-summary-card`/`success`/`pairing-request-host` 骨架/路由注册
- [x] i18n：同桌面（`mobile/src/locales`）

## Phase 3 — web demo（受邀方 only）

- [x] `crates/web` 加 `swarmdrop-invite` 依赖；`WebNode::connect_invite(invite)`（decode → usable_addrs → connect）
- [x] `static/index.html` 删死掉的分享码段、加「粘贴邀请串」框；`client.js` 去 `lookup_share_code`
- [x] 六 crate（现七 crate）wasm 门禁绿；浏览器冒烟：粘贴 desktop/mobile 生成的 invite → 连上

## Phase 4 — 收尾

- [ ] 三端配对冒烟：桌面生成二维码 → 移动扫码 → 双确认 → 配对记录；桌面粘贴移动的邀请
- [x] `simplify-pairing-code` change 归档（已被本线超越，改的文件已不存在）
- [ ] 知识库：net-kernel.md（invite crate + QR 规范）、theme-and-styling.md（QR 不反色规范）
- [ ] `cargo test --workspace` + 七 crate wasm 门禁 + `pnpm exec tsc --noEmit` 全绿
