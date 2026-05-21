<div align="center">

<img src="assets/images/icon.png" width="120" alt="SwarmDrop Mobile">

# SwarmDrop Mobile

**端到端加密的 P2P 文件传输 · React Native 端**

*移动设备之间、跨网络、不经过服务器。*

[![Release](https://img.shields.io/github/v/release/swarm-apps/SwarmDrop-RN?style=flat-square)](https://github.com/swarm-apps/SwarmDrop-RN/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Expo](https://img.shields.io/badge/Expo-SDK%2056-000020?style=flat-square&logo=expo)](https://expo.dev)
[![React Native](https://img.shields.io/badge/React%20Native-0.81-61DAFB?style=flat-square&logo=react)](https://reactnative.dev)

[下载 APK](#下载) · [开发](#开发) · [桌面端](https://github.com/swarm-apps/SwarmDrop)

</div>

---

SwarmDrop 移动客户端，把桌面端的 **跨网络 P2P 文件传输** 体验搬到 Android / iOS。
共享同一个 Rust core（`swarmdrop-core`），通过 [uniffi-bindgen-react-native][ubrn]
桥接到 React Native 的 Turbo Module。

> 这是 [SwarmDrop](https://github.com/swarm-apps/SwarmDrop) 的姐妹仓库；
> 业务定位、安全模型、协议细节看主仓的 README。

<table>
<tr>
<td width="33%" align="center"><sub>🔗</sub><br><b>跨平台配对</b><br><sub>与桌面端互通<br>同一份 P2P 协议</sub></td>
<td width="33%" align="center"><sub>📁</sub><br><b>SAF 自由保存</b><br><sub>Downloads / 用户目录<br>断点续传安全 chunk write</sub></td>
<td width="33%" align="center"><sub>⚡</sub><br><b>自动接收</b><br><sub>已配对设备<br>无需手动确认</sub></td>
</tr>
</table>

## 下载

到 [Releases](https://github.com/swarm-apps/SwarmDrop-RN/releases/latest) 拿 APK。当前只发 Android（aarch64）；
iOS 走 Xcode 自行签名安装，TestFlight 暂未上线。

> 通过 [UpgradeLink](https://upgrade.toolsetlink.com/) 推送应用内升级提示，新版本会主动通知。

## 开发

需要 Node 22+ · pnpm 10+ · Rust 1.80+ · JDK 17 · Android NDK r27c（iOS 还需 Xcode 16+）。

```bash
git clone git@github.com:swarm-apps/SwarmDrop-RN.git
cd SwarmDrop-RN
pnpm install

# 编译 Rust 桥（按平台选一个）
pnpm --filter react-native-swarmdrop-core build:android
pnpm --filter react-native-swarmdrop-core build:ios

# 生成原生工程
pnpm prebuild

# 跑起来
pnpm android        # 或 pnpm ios
```

> Expo Go 不工作 —— 应用要加载原生 Rust 绑定，必须用 dev client。

<details>
<summary><b>Windows 长路径问题</b></summary>

ubrn 在 Windows 下的目标目录路径过长会撞 linker 错误，用 `CARGO_TARGET_DIR` 指到短路径：

```powershell
$env:CARGO_TARGET_DIR='D:\tmp\swarmdrop-mobile-core-target'
pnpm --filter react-native-swarmdrop-core build:android
```

</details>

<details>
<summary><b>本地联调主仓代码</b></summary>

[packages/swarmdrop-core/rust/mobile-core/Cargo.toml][mobile-cargo] 默认走
HTTPS 拉远端主仓（CI 用），本地要改代码联调时改成 path：

```toml
swarmdrop-core = { path = "../../../../../SwarmDrop/crates/core" }
entity         = { path = "../../../../../SwarmDrop/crates/entity" }
migration      = { path = "../../../../../SwarmDrop/crates/migration" }
swarm-p2p-core = { path = "../../../../../SwarmDrop/libs/core" }
```

git 与 path 不能混用（`swarm-p2p-core` 会撞 multiple versions）。

</details>

## 技术栈

| 层 | 技术 |
|---|---|
| 框架 | Expo SDK 56 · React Native 0.81 · expo-router |
| UI | NativeWind v5 (Tailwind) · rn-primitives · lucide-react-native |
| 状态 | Zustand 5 · AsyncStorage 持久化 |
| i18n | Lingui 5（zh · en） |
| 原生桥 | [uniffi-bindgen-react-native][ubrn]（Turbo Module） |
| Rust core | `swarmdrop-core` / `swarm-p2p-core`（与桌面同一份） |
| 文件 I/O | expo-file-system 56（SAF chunk write 支持） |

<details>
<summary><b>仓库结构</b></summary>

```
SwarmDrop-RN/
├── src/                              # RN 业务代码
│   ├── app/                          #   expo-router 路由
│   ├── components/                   #   UI 组件
│   ├── core/                         #   ForeignFileAccess / event-bus / paths
│   └── stores/                       #   zustand stores
├── packages/swarmdrop-core/          # ubrn 包
│   ├── rust/mobile-core/             #   桥层 Rust crate
│   └── src/                          #   生成的 TS 绑定
├── plugins/                          # Expo config plugins
│   ├── with-android-release-signing  #   注入 release.keystore 签名配置
│   └── with-android-install-permission  #   REQUEST_INSTALL_PACKAGES 权限
└── dev-notes/                        # 项目知识库（架构 / 工具链 / 主题色 / 配对传输）
```

</details>

## CI / 发布

- **Build Android**（手动触发）：[workflows/build-android.yml](.github/workflows/build-android.yml)
- **Release**（tag push 触发）：[workflows/release.yml](.github/workflows/release.yml)
  - git-cliff 生成 changelog → ubrn release build → 签名 APK → draft release → publish → UpgradeLink 同步

发布新版本：

```bash
# 编辑 package.json + app.json 的 version
git commit -am "chore(release): vX.Y.Z"
git tag vX.Y.Z && git push origin main && git push origin vX.Y.Z
```

## License

[MIT](LICENSE) &copy; SwarmDrop Contributors

<div align="center"><sub>Built with <a href="https://expo.dev">Expo</a> · <a href="https://github.com/jhugman/uniffi-bindgen-react-native">uniffi-bindgen-react-native</a></sub></div>

[ubrn]: https://github.com/jhugman/uniffi-bindgen-react-native
[mobile-cargo]: packages/swarmdrop-core/rust/mobile-core/Cargo.toml
