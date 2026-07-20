# 移动端策略

## 问题背景

rust-libp2p 官方不支持移动平台，在 Android/iOS 上存在以下已知问题：

1. **mDNS 限制**
   - Android 14+ (API 34) 才支持 subtypes
   - iOS 需要 Bonjour 权限和后台模式配置
   - 移动端 Wi-Fi 休眠会导致 mDNS 失效

2. **后台运行限制**
   - iOS 严格限制后台网络活动
   - Android 省电模式会杀死长连接
   - 需要前台服务或特殊权限

3. **编译兼容性**
   - rust-libp2p 部分依赖可能不支持移动端 target
   - tokio 在移动端需要特殊配置

## 推荐策略：桌面优先 + 移动端轻量方案

### Phase 1-3: 专注桌面端

保持现有计划，使用 rust-libp2p 实现完整功能：
- Windows / macOS / Linux
- 完整的 mDNS + DHT + Relay + DCUtR

### Phase 4+: 移动端采用轻量方案

#### 方案 A：HTTP Relay（推荐）

```
移动端架构：
┌─────────────────┐     HTTP/WebSocket     ┌─────────────────┐
│   Mobile App    │ ◄──────────────────────► │   Desktop App   │
│   (Tauri 2)     │                          │   (libp2p)      │
└─────────────────┘                          └─────────────────┘
        │
        │ 当需要跨网络时
        ▼
┌─────────────────┐
│  Relay Server   │  ← 轻量 HTTP 中继
│  (自建/公共)     │
└─────────────────┘
```

**优点**：
- 移动端代码简单，无需 libp2p
- HTTP 在移动端有良好支持
- 可利用系统级推送通知

**实现**：
```rust
// 桌面端同时启动 HTTP Server
#[tauri::command]
async fn start_local_http_server() -> Result<String, String> {
    // 返回 http://192.168.x.x:port
}

// 移动端通过 HTTP 发现和传输
async fn discover_desktop_via_http(local_ip: &str) -> Vec<Device>;
async fn transfer_via_http(file: File, target: &str) -> Result<(), Error>;
```

#### 方案 B：WebRTC Data Channel

```
┌─────────────────┐     WebRTC      ┌─────────────────┐
│   Mobile App    │ ◄──────────────► │   Desktop App   │
└─────────────────┘                  └─────────────────┘
        │                                    │
        └──────── Signaling Server ──────────┘
```

**优点**：
- 真正的 P2P 连接
- NAT 穿透内置
- 移动端浏览器支持好

**缺点**：
- 需要信令服务器
- Rust WebRTC 库不太成熟

#### 方案 C：gomobile-libp2p

使用 Go 的 libp2p 实现，通过 FFI 桥接：

```
┌─────────────────────────────────┐
│         Tauri Mobile App        │
├─────────────────────────────────┤
│     Rust ◄──FFI──► Go Library   │
│                    (libp2p)     │
└─────────────────────────────────┘
```

**优点**：
- 功能完整，与桌面端协议兼容
- 有 gomobile-ipfs 参考实现

**缺点**：
- 构建复杂度高
- 需要维护 FFI 绑定
- 包体积增大

---

## 建议实施顺序

```
阶段 1-3: 桌面端完整实现 (rust-libp2p)
    ↓
阶段 4: 移动端 MVP (方案 A: HTTP Relay)
    - 局域网：HTTP 直连
    - 跨网络：通过公共 Relay
    ↓
阶段 5: 评估是否需要完整 P2P
    - 如果用户反馈需要：考虑方案 B 或 C
    - 如果 HTTP 够用：保持简单
```

---

## 局域网传输：移动端简化方案

针对最常见的局域网场景，可以用简单的 HTTP 方案：

### 桌面端（发送方）

```rust
// 在已有的 libp2p 基础上，额外启动 HTTP server
use axum::{Router, routing::get};

async fn start_http_transfer_server(files: Vec<PathBuf>) -> String {
    let app = Router::new()
        .route("/files", get(list_files))
        .route("/files/:id", get(download_file))
        .route("/upload", post(receive_file));

    // 绑定到局域网 IP
    let addr = "0.0.0.0:19528";
    axum::Server::bind(&addr).serve(app).await;

    format!("http://{}:19528", local_ip())
}
```

### 移动端（接收方）

```typescript
// 简单的 HTTP 客户端
async function discoverDesktopDevices(): Promise<Device[]> {
  // 方法 1: 扫描局域网 (慢)
  // 方法 2: 用户手动输入 IP
  // 方法 3: 扫描二维码获取地址
}

async function downloadFile(url: string, fileId: string) {
  const response = await fetch(`${url}/files/${fileId}`);
  // 使用 Tauri 文件 API 保存
}
```

### 跨网络传输：公共 Relay

```
移动端 ──HTTP──► 公共 Relay Server ◄──HTTP── 桌面端

Relay Server 功能：
- 接收上传的文件（临时存储）
- 生成下载链接
- 端到端加密（密钥在分享码中）
- 文件自动过期删除
```

---

## 风险评估更新

| 风险 | 原方案影响 | 新方案对策 |
|------|-----------|-----------|
| rust-libp2p 移动端不可用 | 阻塞移动端开发 | 采用 HTTP 方案绕过 |
| mDNS 在移动端受限 | 局域网发现失效 | 使用二维码/手动输入/UDP 广播 |
| 后台传输受限 | 大文件传输可能中断 | 分块 + 断点续传 + 前台服务 |

---

## 产品调整

### MVP 范围缩减

- **MVP 目标**：Windows / macOS / Linux 桌面端
- **移动端**：降级为 P1，使用 HTTP 简化方案

### 新增功能：二维码配对

由于移动端 mDNS 不可靠，增加二维码配对方式：

```
桌面端显示二维码 → 移动端扫码 → 获取连接信息 → 直接 HTTP 连接
```

二维码内容：
```json
{
  "type": "swarmdrop",
  "ip": "192.168.1.100",
  "port": 19528,
  "session_id": "abc123",
  "encryption_key": "base64..."
}
```

---

## 结论

1. **桌面端**：继续使用 rust-libp2p，功能完整
2. **移动端**：采用 HTTP 简化方案，降低技术风险
3. **用户体验**：通过二维码配对弥补 mDNS 的不足
4. **未来**：根据用户反馈决定是否投入完整 P2P 移动端
