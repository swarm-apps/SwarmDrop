# 移动端 libp2p 可行性调研

## 背景

发现 [Fungi](https://github.com/enbop/fungi) 项目成功在 Android 上使用了 rust-libp2p。

## Fungi 的方案

```
架构: Flutter (UI) + Rust Daemon (libp2p) + gRPC (通信)
```

- UI 用 Flutter 实现跨平台
- Rust daemon 独立运行，负责 libp2p 网络
- 通过 gRPC 解耦 UI 和网络层

## Tauri 2 的能力

根据 [Tauri Mobile Plugin Development](https://v2.tauri.app/develop/plugins/develop-mobile/)：

> "While Tauri doesn't directly provide a mechanism to call Rust from your plugin code, using **JNI on Android** and **FFI on iOS** allows plugins to call shared code."

这意味着 **Tauri 2 移动端可以调用 Rust 库**，包括 libp2p。

## 重新评估问题

### 之前的担忧

1. rust-libp2p 官方不支持移动端
2. mDNS 在移动端受限
3. 后台运行会被杀死

### 实际情况

| 功能 | 可行性 | 说明 |
|------|--------|------|
| TCP + Noise + Yamux | ✅ 可行 | 标准网络，无特殊限制 |
| Kademlia DHT | ✅ 可行 | 无平台限制 |
| Relay | ✅ 可行 | 无平台限制 |
| DCUtR | ✅ 可行 | UDP 打洞，应该可以 |
| mDNS | ⚠️ 受限 | 需要权限，后台受限 |
| 后台运行 | ⚠️ 受限 | 需要前台服务 |

### 核心洞察

**libp2p 的大部分功能在移动端是可以工作的**，问题主要在：

1. **mDNS 发现**：移动端后台 mDNS 受限，但可以：
   - 前台时正常工作
   - 用二维码/分享码替代
   - 用 DHT 替代（跨网络）

2. **后台运行**：传输过程中需要保持 App 前台
   - 可接受（传输时看进度很正常）
   - 可用前台服务通知

---

## 修订后的方案

### 方案 D：Tauri 2 + libp2p 全平台（新增）

```
┌─────────────────────────────────────────────────────────────────┐
│                     统一架构                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   桌面端 & 移动端 共用:                                          │
│   ┌───────────────────────────────────────────────────────┐    │
│   │  Tauri 2 + React                                      │    │
│   │  └── Rust 后端                                        │    │
│   │       └── libp2p (跨平台)                             │    │
│   │           ├── TCP + Noise + Yamux                     │    │
│   │           ├── Kademlia DHT                            │    │
│   │           ├── Relay + DCUtR                           │    │
│   │           ├── Request-Response                        │    │
│   │           └── mDNS (桌面端为主，移动端可选)            │    │
│   └───────────────────────────────────────────────────────┘    │
│                                                                 │
│   移动端特殊处理:                                                │
│   ├── 二维码配对（替代/补充 mDNS）                              │
│   ├── 传输时保持前台                                            │
│   └── Android 前台服务通知                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 优势

| 对比项 | 方案 C (HTTP) | 方案 D (libp2p) |
|--------|--------------|-----------------|
| 代码复用 | 低（两套协议）| 高（统一协议）|
| 移动端之间传输 | ❌ 需要桌面中转 | ✅ 直接传输 |
| 跨网络 NAT 穿透 | 需要 Relay | libp2p 内置 |
| 维护成本 | 中 | 低 |
| 实现复杂度 | 低 | 中 |

### 风险

| 风险 | 对策 |
|------|------|
| libp2p 编译问题 | 先做 PoC 验证 |
| 某些功能不工作 | 功能降级，保留 HTTP 备选 |
| 性能问题 | 基准测试，优化 |

---

## 建议的实施路径

```
Phase 1-3: 桌面端 MVP（不变）
    │
    ▼
验证阶段: libp2p 移动端 PoC
    ├── 创建 Tauri 2 移动端测试项目
    ├── 集成 libp2p 基础功能
    ├── 测试 TCP/DHT/Relay 连接
    ├── 测试 Android + iOS 编译
    └── 评估结果
    │
    ▼
Phase 4: 根据 PoC 结果决定
    ├── 成功 → 方案 D (libp2p 全平台)
    └── 失败 → 方案 C (HTTP 备选)
```

---

## PoC 验证清单

### 1. 环境准备

```bash
# Android
rustup target add aarch64-linux-android armv7-linux-androideabi
# iOS (macOS only)
rustup target add aarch64-apple-ios x86_64-apple-ios
```

### 2. 最小 libp2p 依赖测试

```toml
# 测试最小依赖能否编译
[dependencies]
libp2p = { version = "0.54", features = [
    "tokio",
    "tcp",
    "noise",
    "yamux",
    "identify",
    "kad",        # DHT
    "relay",      # Relay
    "dcutr",      # NAT 穿透
    # "mdns",     # 先不测试 mDNS
] }
```

### 3. 测试项

- [ ] Android aarch64 编译通过
- [ ] Android armv7 编译通过
- [ ] iOS aarch64 编译通过
- [ ] iOS x86_64 (模拟器) 编译通过
- [ ] Android 真机运行，能连接引导节点
- [ ] Android 真机，两设备能互相发现（DHT）
- [ ] Android 真机，能通过 Relay 传输数据
- [ ] iOS 真机测试（需要 macOS + Apple Developer）

### 4. 预期问题

| 问题 | 可能原因 | 解决方案 |
|------|----------|----------|
| 编译失败 | 某些 crate 不支持目标 | 找替代 crate 或 feature flag |
| 链接错误 | NDK 版本问题 | 升级 NDK |
| 运行时崩溃 | tokio 配置问题 | 调整 runtime 配置 |
| 连接失败 | 权限问题 | 配置 AndroidManifest |

---

## 结论

**libp2p 移动端并非完全不可行**，而是需要：

1. 实际验证编译和运行
2. 接受 mDNS 在移动端的限制
3. 用二维码/DHT 补充发现机制
4. 传输时保持 App 前台

**建议**：
- 在 Phase 3 完成后，花 1-2 天做 PoC 验证
- 根据结果决定 Phase 4 采用哪个方案
