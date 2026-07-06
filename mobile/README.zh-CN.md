<a name="readme-top"></a>

<div align="center">

<img src="assets/images/readme-icon.png" width="140" alt="SwarmDrop Mobile logo">

# SwarmDrop Mobile

### 跨网络、端到端加密的文件传输 —— 装进口袋。

[SwarmDrop](https://github.com/swarm-apps/SwarmDrop) 的 Android 与 iOS 客户端，
与桌面端共享同一份 Rust core。

[![Release](https://img.shields.io/github/v/release/swarm-apps/SwarmDrop-RN?style=flat-square)](https://github.com/swarm-apps/SwarmDrop-RN/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platform-Android%20%7C%20iOS-lightgrey?style=flat-square)](#-下载)

[![Expo](https://img.shields.io/badge/Expo-SDK%2056-000020?style=flat-square&logo=expo)](https://expo.dev)
[![React Native](https://img.shields.io/badge/React%20Native-0.85-61DAFB?style=flat-square&logo=react)](https://reactnative.dev)

[English](README.md) · **简体中文**

[官网](https://swarm-apps.github.io/SwarmDrop/) · [特性](#-特性) · [下载](#-下载) · [开发](#-开发) · [桌面端](https://github.com/swarm-apps/SwarmDrop)

</div>

<!-- TODO(资产): 此处放一段演示 GIF —— 与桌面端配对、手机收到文件，约 12s。
     建议路径：docs/screenshots/mobile-demo.gif -->

---

## 简介

SwarmDrop Mobile 把 SwarmDrop 的**跨网络 P2P 文件传输**带到 Android 与 iOS：在你的任意设备之间收发文件，跨任意网络，且**只有收发双方能解密** —— 无账号、无服务器。

它与桌面端共享**同一份 Rust core**（`swarmdrop-core`），通过 [uniffi-bindgen-react-native][ubrn] 桥接成 React Native 的 Turbo Module。同一套加密协议、同一套配对 —— 它就是能直接跟你的电脑对话。

> 这是 [**SwarmDrop**](https://github.com/swarm-apps/SwarmDrop) 的姐妹仓库。产品定位、安全模型与协议细节，见主仓的 README。

## ✨ 特性

| | | |
|---|---|---|
| 🔗 **跨平台配对** | 与桌面端用完全相同的 P2P 协议配对、传输。 | <sub>手机 ↔ 电脑，跨网络</sub> |
| 🔒 **端到端加密** | XChaCha20-Poly1305，每次传输独立密钥；中继看不到明文。 | <sub>与桌面端同款加密</sub> |
| 📁 **自由保存（SAF）** | 可写入 Downloads 或任意用户指定目录，断点续传 + 安全分块写入。 | <sub>Android 存储访问框架</sub> |
| ⚡ **自动接收** | 已配对设备发来的文件后台落盘，无需手动点确认。 | <sub>契合被动接收模型</sub> |

<div align="right"><a href="#readme-top">↑ 回到顶部</a></div>

## 📦 下载

**[前往官网下载 →](https://swarm-apps.github.io/SwarmDrop/)** —— 桌面与移动端，全平台一站获取。

| 平台 | 格式 |
|---|---|
| **Android** | `.apk` —— [SwarmDrop-RN releases](https://github.com/swarm-apps/SwarmDrop-RN/releases/latest)（arm64-v8a） |
| **iOS** | 源码构建（见[开发](#-开发)）；TestFlight 暂未上线 |

> 🔄 应用内**自动更新**由 [**SwarmHive**](https://github.com/swarm-apps/SwarmHive) 提供 —— 与桌面端同一套可自托管的开源更新服务，不依赖任何商业更新 SaaS。

<div align="right"><a href="#readme-top">↑ 回到顶部</a></div>

## 🛠 开发

需要 **Node 22+** · **pnpm 10+** · 较新的稳定版 **Rust**（1.85+）· **JDK 17** · **Android NDK r27c**（iOS 还需 **Xcode 16+**）。

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
pnpm android        # 或：pnpm ios
```

> Expo Go 不工作 —— 应用要加载原生 Rust 绑定，必须用 dev client。

<details>
<summary><b>Windows 长路径问题</b></summary>

<br>

ubrn 在 Windows 下的目标目录路径过长会撞 linker 错误，用 `CARGO_TARGET_DIR` 指到短路径：

```powershell
$env:CARGO_TARGET_DIR='D:\tmp\swarmdrop-mobile-core-target'
pnpm --filter react-native-swarmdrop-core build:android
```

</details>

<details>
<summary><b>本地联调主仓代码</b></summary>

<br>

[`packages/swarmdrop-core/rust/mobile-core/Cargo.toml`][mobile-cargo] 默认走 HTTPS 拉远端主仓（CI 用）。本地要改 core 联调时改成 path 依赖：

```toml
swarmdrop-core = { path = "../../../../../SwarmDrop/crates/core" }
entity         = { path = "../../../../../SwarmDrop/crates/entity" }
migration      = { path = "../../../../../SwarmDrop/crates/migration" }
swarm-p2p-core = { path = "../../../../../SwarmDrop/libs/core" }
```

git 与 path 不能混用（`swarm-p2p-core` 会撞 multiple versions）。

</details>

<details>
<summary><b>技术栈</b></summary>

<br>

| 层 | 技术 |
|---|---|
| 框架 | Expo SDK 56 · React Native 0.85 · React 19 · expo-router |
| UI | NativeWind v5 (Tailwind) · rn-primitives · lucide-react-native |
| 状态 | Zustand 5 · AsyncStorage 持久化 |
| i18n | Lingui 5（zh · en） |
| 原生桥 | [uniffi-bindgen-react-native][ubrn]（Turbo Module） |
| Rust core | `swarmdrop-core` / `swarm-p2p-core`（与桌面同一份） |
| 文件 I/O | expo-file-system 56（SAF 分块写入） |
| 更新 | SwarmHive 引擎（registry-rn） |

</details>

<details>
<summary><b>仓库结构</b></summary>

<br>

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
├── plugins/                          # Expo config plugins（release 签名、安装权限）
└── dev-notes/                        # 项目知识库
```

</details>

<div align="right"><a href="#readme-top">↑ 回到顶部</a></div>

## 🚢 CI / 发布

- **Build Android**（手动触发）：[`workflows/build-android.yml`](.github/workflows/build-android.yml)
- **Release**（tag push 触发）：[`workflows/release.yml`](.github/workflows/release.yml)
  - git-cliff 生成 changelog → ubrn release build → 签名 APK → GitHub draft release → publish → **SwarmHive**（上传 + finalize → `stable` 渠道）

发布新版本：

```bash
# 编辑 package.json + app.json 的 version
git commit -am "chore(release): vX.Y.Z"
git tag vX.Y.Z && git push origin main && git push origin vX.Y.Z
```

## ❤️ swarm-apps 家族

一系列去中心化、本地优先、端到端加密工具中的一员：

- **SwarmDrop** —— 设备间文件传输。[桌面端](https://github.com/swarm-apps/SwarmDrop) · [移动端](https://github.com/swarm-apps/SwarmDrop-RN)
- **SwarmNote** —— 去中心化的加密笔记。[桌面端](https://github.com/swarm-apps/SwarmNote) · [移动端](https://github.com/swarm-apps/SwarmNote-RN)
- **SwarmHive** —— 可自托管的开源发布与自动更新服务，支持 Tauri 与 React Native 应用。[仓库](https://github.com/swarm-apps/SwarmHive)

## 📄 许可证

[MIT](LICENSE) © SwarmDrop Contributors

<div align="center"><sub>Built with <a href="https://expo.dev">Expo</a> · <a href="https://github.com/jhugman/uniffi-bindgen-react-native">uniffi-bindgen-react-native</a></sub></div>

<div align="right"><a href="#readme-top">↑ 回到顶部</a></div>

[ubrn]: https://github.com/jhugman/uniffi-bindgen-react-native
[mobile-cargo]: packages/swarmdrop-core/rust/mobile-core/Cargo.toml
