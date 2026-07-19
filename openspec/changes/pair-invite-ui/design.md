# pair-invite-ui 设计决策

基于 2026-07-19 四路 UI 技术栈调研（本仓 workflow：移动扫码 / 三端 QR / 深链 / 现有 UI 盘点）
+ 用户逐项确认。技术栈论断带版本号，原始数据在会话 workflow 产出。

## D1：QR 生成放 Rust core-family（`crates/invite`，用户确认）

**否决** per-end JS 库三套（qrcode.react + react-native-qrcode-svg + fast_qr-wasm）。理由：

- 契合刚建的「core 单源、三端薄壳」架构（core-wasm-ready 的延续）
- **大写 alphanumeric + ECL::M + quiet zone 策略单点固化**——三套库要三处各写一遍、易漂移
- QR 与它编码的邀请串同源同 crate（单一职责）

实现：`crates/invite` 用 `fast_qr@0.13.1`（wasm-first，`SvgBuilder`）导出
`pair_invite_qr_svg(invite: &str) -> String`（或返回模块矩阵 `Vec<Vec<bool>>` 让三端自绘）。
三端渲染：web 直接 `el.innerHTML = svg`；桌面 React `dangerouslySetInnerHTML` 或按矩阵画
`<rect>`；移动 `react-native-svg`（Expo 56 内置 15.15.4）按矩阵画 `<Rect>`（~30 行组件）。
代价：桌面/移动各手写 ~30 行渲染，非现成组件——换来策略单点 + 三端逐像素一致。

## D2：QR 编码规范（三端统一，写进实现）

调研关键发现（跨两路印证）：邀请串规范形态是**小写** base32（`invite.rs` `make_ascii_lowercase`），
只能走 QR byte 模式（8 bit/字符）→ v13-15。而 base32 大写字母表 `A-Z2-7` 100% 落在 QR
alphanumeric 字符集，**编码前 `.to_ascii_uppercase()` → alphanumeric 模式（5.5 bit/字符）→
v11-12，模块数 -15%，扫码可靠性显著↑**。解码大小写不敏感（`decode` 内 `to_ascii_uppercase`）
→ **零风险**。QR 编码器（fast_qr / 所有主流库）自动 segment 检测，只需喂大写串。

- **模式**：payload 大写 → alphanumeric（本 crate `pair_invite_qr_svg` 内部做，调用方传小写规范串）
- **ECL**：M（15%，屏→摄像头近距干净场景足够；Q/H 只会顶高版本更难扫；无 logo 不需要）
- **quiet zone**：4 模块（ISO 硬性；`fast_qr` 的 margin）
- **配色**：深模块 `#0a0a0a` + 白底 `#ffffff`，**不随暗色主题反色**（摄像头对反色 QR 识别差），
  套白色圆角卡
- **屏显**：≥260px（v12 约 65 模块，每模块 ≥3-4 设备像素）
- 始终提供「复制/粘贴链接」通道兜底（长串扫码失败率不可忽略）

## D3：移动扫码 = expo-camera CameraView（用户不需要选，调研定论）

`expo-camera@56.0.0` 的 `<CameraView barcodeScannerSettings={{barcodeTypes:["qr"]}}
onBarcodeScanned={cb}/>`（SDK53+ 内置扫码）。**否决** vision-camera（杀鸡用牛刀、引入
dev-client + MLKit 包体、纯扫码无收益）；`expo-barcode-scanner` 已于 SDK52 删除。

- 权限：`useCameraPermissions()` 三态 + config plugin 写 `NSCameraUsageDescription`/Android
  `CAMERA`（只扫码不录音，不要 microphone/RECORD_AUDIO）
- **UX**：不一进页面就弹系统权限框——先 primer「扫码用于建立配对，仅本地用相机」→ 用户点
  「开启相机」再 `requestPermission()`；`canAskAgain===false` 时引导 `Linking.openSettings()`
- **扫码与粘贴并列两入口**（非扫码失败才降级）——粘贴走已装 `expo-clipboard.getStringAsync()`
- **去抖**：长 QR 连发多次 `onBarcodeScanned`，扫到即上锁（`locked ? undefined : handle`）+
  校验 `sdinvite` 前缀
- ⚠️ 坑记录：SDK54/55 有 barcode 静默禁用 bug（expo #44491，ZXing 未编入），**SDK56 已修**，
  本项目在 56 无需 workaround；加 expo-camera 后须 `expo prebuild` 原生重编

## D4：深链本期不做（用户确认）

本期三端入口 = 二维码 + 复制链接 + 剪贴板感知/粘贴/扫码。深链（`swarmdrop://pair/sdinvite<base32>`
自定义 scheme、path 非 fragment、免域名）作后续独立 change——它需要 `tauri-plugin-deep-link@2.4.9`
+ macOS 上深链与 share-target 都钩 `RunEvent::Opened` 的分流 PoC（`external_open.rs`），风险隔离。
剪贴板感知（D5）已给「复制邀请→回应用→一键配对」的顺滑，深链是锦上添花。

## D5：剪贴板感知 = 「感知 + 一键确认」（承接 pair-invite-protocol design D7）

见 pair-invite-protocol/design.md D7。三端隐私模型差异：桌面 focus 静默读、iOS
`Clipboard.hasStringAsync()` 只探不读（不弹横幅）亮 chip、Android 读+toast、web 粘贴按钮。
前缀秒判 + 本地 decode 验签 → 确认卡（对端名/平台/短指纹）→ 用户点确认才发起（安全闸，
邀请是信任凭证不全自动）。

## D6：移动补回 Direct 配对（用户确认，两端对称）

移动 rust（mobile-core/pairing.rs）在配对码下线时删了 `request_pairing`，导致 LAN 近场
`handlePair`（列表点设备 direct 直连）失去后端。补回一个 uniffi 方法
`pair_direct(peer_id) -> MobilePairingResult`（委托 core `request_pairing(peer_id,
PairingMethod::Direct, None)`）。移动保留两条路径：LAN 近场 direct + 扫码/粘贴 invite，
与桌面 `directPairing` + invite 对称。

## D7：invite 下沉 `crates/invite`（技术细节，按推荐）

`pairing/invite.rs` 只依赖 `net-base` 类型（Addr/NodeAddr/NodeId/SecretKey）+
sha2/postcard/data-encoding——净 wasm-clean。下沉到新 `crates/invite`（不放 net-base，避免
「类型底座」crate 沾 sha2/postcard/fast_qr）。core `pairing/manager.rs` 与 web 都依赖它。
QR 生成（D1）也住这里。新 crate 进 check-wasm 门禁（第七个）。invite 单测随迁。
