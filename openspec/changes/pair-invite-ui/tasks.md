# pair-invite-ui 任务分解

## Phase 0 — invite 下沉 crates/invite + QR 生成（D1/D2/D7）

- [ ] 新建 `crates/invite`（swarmdrop-invite），workspace 登记；`pairing/invite.rs` 迁入（PairInvite/InviteRegistry/wire + 单测），依赖 net-base + sha2/postcard/data-encoding
- [ ] QR 模块：`fast_qr` 生成 `pair_invite_qr_svg(&str) -> String`（内部 `.to_ascii_uppercase()` + ECL::M + 4 模块 quiet zone + 深/白配色）；或 `qr_matrix(&str) -> Vec<Vec<bool>>` 供三端自绘
- [ ] core `pairing/manager.rs` 改从 `swarmdrop-invite` 引入；删 `crates/core/src/pairing/invite.rs`
- [ ] `crates/invite` 进 `scripts/check-wasm.sh`（第七 crate），双 target 绿；`cargo test -p swarmdrop-invite`
- [ ] QR 大写→版本降档单测（v13-15 → v11-12）；`cargo test --workspace` 全绿

## Phase 1 — 桌面 React UI

- [ ] **前置修复**：`pairing-store.ts` 去掉 `PairingCodeInfo`/`generatePairingCode`/`getDeviceInfo`/`searchDevice`/`found` 态，`pnpm exec tsc --noEmit` 恢复通过
- [ ] QR 渲染组件（消费 core 出的 SVG/矩阵，白卡包裹，≥280px）
- [ ] 发起方屏：`generatePairInvite(localOnly)` → 二维码 + 复制链接 + 5min 倒计时 + LocalOnly 开关 + 重新生成
- [ ] 受邀方屏：粘贴框 + 剪贴板感知（focus 静默读 + `sdinvite` 前缀 → 顶部一键条）→ `consumePairInvite`
- [ ] 删 `generate.lazy.tsx`/`input.lazy.tsx` 6 位码屏、`add-device-section` 码面板 + `PairingInputDialog`
- [ ] 复用校验：`connection-request-dialog`/`device-icon`/入站 store 分支/`directPairing`/`use-pairing-success`/`task-surface`
- [ ] i18n：废码文案 + 新增邀请文案，`pnpm i18n:extract`

## Phase 2 — 移动 RN UI

- [ ] mobile rust 补 `pair_direct(peer_id)`（D6）；`pnpm --filter react-native-swarmdrop-core build:ios` 重建绑定
- [ ] `expo install expo-camera` + `app.json` config plugin（cameraPermission 文案）+ `expo prebuild` 重编
- [ ] QR 渲染组件（`react-native-svg` 按矩阵画 `<Rect>`）
- [ ] 发起方卡/屏：`generatePairInvite` → 二维码 + 复制（`expo-clipboard`）+ 倒计时
- [ ] 受邀方：`CameraView` 扫码（qr 过滤 + 去抖 + 前缀校验 + 权限 primer + `openSettings` fallback）+ 粘贴 + 剪贴板感知（`hasStringAsync` 探测亮 chip）→ `consumePairInvite`
- [ ] 删 `pairing-code-store`、`PairingCodeCard/Sheet/Input`、`lookupDeviceByCode` 链；`respondPairingRequest` 去 code 参数；LAN 近场 `handlePair` 接 `pairDirect`
- [ ] 复用：`peer-summary-card`/`success`/`pairing-request-host` 骨架/路由注册
- [ ] i18n：同桌面（`mobile/src/locales`）

## Phase 3 — web demo（受邀方 only）

- [ ] `crates/web` 加 `swarmdrop-invite` 依赖；`WebNode::connect_invite(invite)`（decode → usable_addrs → connect）
- [ ] `static/index.html` 删死掉的分享码段、加「粘贴邀请串」框；`client.js` 去 `lookup_share_code`
- [ ] 六 crate（现七 crate）wasm 门禁绿；浏览器冒烟：粘贴 desktop/mobile 生成的 invite → 连上

## Phase 4 — 收尾

- [ ] 三端配对冒烟：桌面生成二维码 → 移动扫码 → 双确认 → 配对记录；桌面粘贴移动的邀请
- [ ] `simplify-pairing-code` change 归档（已被本线超越，改的文件已不存在）
- [ ] 知识库：net-kernel.md（invite crate + QR 规范）、theme-and-styling.md（QR 不反色规范）
- [ ] `cargo test --workspace` + 七 crate wasm 门禁 + `pnpm exec tsc --noEmit` 全绿
