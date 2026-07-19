## Why

PairInvite 协议内核已落地（openspec: pair-invite-protocol——net-base sign/verify、`pairing/invite.rs` 编码+Registry、协议接入、6 位配对码整体废弃）。但三端 UI 仍停在配对码时代：

- **桌面**：bindings 已重生成（含 `generatePairInvite`/`consumePairInvite`），但前端 TS **当前 tsc 不过**——`src/stores/pairing-store.ts` 仍 import 已删的 `PairingCodeInfo`、调不存在的 `generatePairingCode`/`getDeviceInfo`。这是仓库里躺着的编译错误。
- **移动**：uniffi 绑定**未重生成**（仍是旧的 `generatePairingCode`/`lookupDeviceByCode`），RN UI 全套 6 位码流程。且移动 rust 删 `request_pairing` 后，LAN 近场直连 `handlePair` 失去后端方法（桌面仍保留 direct）。
- **web**：demo 的「分享码 lookup」段已是死代码（`WebNode` 无此方法）。

三端 UI 一起做——发起方展示邀请二维码 + 复制链接 + 倒计时，受邀方扫码（移动）/ 粘贴 / 剪贴板感知一键配对。web 做受邀方 demo（完整 web UI 后续交外部开发者）。

2026-07-19 四路技术栈调研（本仓 workflow）定了栈与四个决策。

## What Changes

- **新 crate `crates/invite`（swarmdrop-invite，wasm-clean）**：`pairing/invite.rs` 整体下沉（PairInvite 编解码/wire、InviteRegistry），依赖 `net-base` + sha2/postcard/data-encoding（不依赖 core）。**QR 生成收进本 crate**（`fast_qr` → SVG 字符串 / 模块矩阵），大写+ECL::M+quietzone 策略单点固化。core 的 `pairing/manager.rs` 改从 `swarmdrop-invite` 引入；web 依赖它做 decode + QR。
- **QR 三端统一规范**：喂编码器前 `payload.to_ascii_uppercase()` → alphanumeric 模式（v13-15 降 v11-12，扫码可靠性↑，解码大小写不敏感零风险）；ECL::M；4 模块 quiet zone；深模块+白底不随暗色反色；屏显 ≥260px。三端渲染 core 出的 SVG/矩阵（~30 行组件）。
- **桌面 React UI**：删 `generate.lazy.tsx`（6 位码）/`input.lazy.tsx`（OTP+searchDevice）/`add-device-section` 的码面板+`PairingInputDialog`；重写 `pairing-store` 去掉 activeCode/searchDevice/found 态；新建发起方屏（二维码 + 复制链接 + 5min 倒计时 + LocalOnly 开关 + 重新生成）+ 受邀方屏（粘贴/剪贴板感知 → `consumePairInvite`）。复用 `connection-request-dialog`/`device-icon`/入站 store 分支/`directPairing`/`task-surface`。
- **移动 RN UI**：先 `pnpm --filter react-native-swarmdrop-core build:ios` 重建绑定；加 `expo-camera`（扫码）+ QR 渲染（react-native-svg 已内置）；删 `pairing-code-store`/OTP 链；新建发起方卡（二维码+复制）+ 受邀方（`CameraView` 扫码 + 粘贴 + 剪贴板感知，权限 primer + 粘贴 fallback）；**mobile rust 补回 direct 配对方法**保持与桌面对称（LAN 近场 `handlePair` 恢复）。复用 `peer-summary-card`/`success`/`pairing-request-host`。
- **web demo**：`WebNode::connect_invite(invite)`（`PairInvite::decode` 纯函数取 inviter + usable_addrs → 复用 `connect`）；`static/index.html` 删死掉的分享码段、加「粘贴邀请串」框。受邀方 only。
- **剪贴板感知（D7，已定）**：三端「感知 + 一键确认」——桌面 focus 静默读、iOS `hasStringAsync` 探测亮 chip、Android 读+toast、web 粘贴按钮。前缀 `sdinvite` 秒判，读后本地 decode 验签 → 确认卡。
- **i18n**：废 6 位码文案、新增邀请/二维码/仅本地网络文案，`pnpm i18n:extract`。
- **非目标**：深链（`swarmdrop://`——本期不做，剪贴板感知已给「复制→回来→配对」，深链 + macOS/share-target 分流 PoC 作后续独立 change）；web 完整 invite 握手（web 消费 core 大工程）；Universal Link/二维码 logo。

## Capabilities

### New Capabilities
- `pair-invite-ui`: 三端配对界面——发起方生成并展示邀请二维码（大写 alphanumeric、ECL::M）+ 复制链接 + TTL 倒计时；受邀方扫码（移动相机）/ 粘贴 / 剪贴板感知一键，本地验签后确认对端身份并发起配对。

### Modified Capabilities
- `pairing`: 移动端补回 `Direct` 配对入口（`request_pairing` 的移动 uniffi 对等方法），恢复 LAN 近场点按直连，与桌面对称。

## Impact

- **新增 `crates/invite`**：invite.rs 迁入 + QR 模块 + Cargo.toml（依赖 net-base/sha2/postcard/data-encoding/fast_qr）。core/web 加依赖；`crates/core/src/pairing/invite.rs` 删除（改 re-export 或直接引用 swarmdrop-invite）。
- **桌面前端**：`pairing-store.ts` 重写、`routes/_app/pairing/*` 重写、`add-device-section.tsx` 改、新 QR 渲染组件、剪贴板感知 hook；bindings 已就绪。**修复当前 tsc 失败是本 change 前置**。
- **移动**：uniffi 重建、`app.json` 加 expo-camera plugin、`react-native-svg`（已内置）QR 组件、`pairing-code-store` 删、配对屏重写、mobile rust 补 direct 方法 + 重生成绑定。
- **web**：`crates/web/src/node.rs` 加 `connect_invite`、`static/index.html` 改。
- **回归**：`cargo test --workspace`（invite 单测随迁移到新 crate）+ 六 crate wasm 门禁（新 crate 进门禁）；桌面 `pnpm exec tsc --noEmit` 恢复通过；桌面/移动配对冒烟（生成→扫码/粘贴→确认→配对记录）。
- **风险**：QR 矩阵渲染组件三端各手写（~30 行×2，桌面+移动）——需视觉一致性校对；移动 `expo-camera` 加原生模块须 prebuild 重编；`crates/invite` 进 wasm 门禁需确认 fast_qr wasm 洁净。
