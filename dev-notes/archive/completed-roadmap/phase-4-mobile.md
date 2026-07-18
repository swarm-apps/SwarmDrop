# Phase 4: 移动端 MVP

> **范围**: iOS / Android
>
> **参考**: [implementation-roadmap.md](implementation-roadmap.md) 查看整体规划

## 目标

实现移动端与桌面端之间的文件传输，采用 HTTP/WebSocket 方案（不使用 libp2p）。

## 技术决策

### 为什么不用 libp2p？

| 问题 | 说明 |
|------|------|
| rust-libp2p 移动端不成熟 | 官方不支持，社区优先级低 |
| mDNS 移动端受限 | Android/iOS 后台限制严重 |
| 后台运行被杀 | 普通 App 无法长期后台运行 |

### 为什么用 HTTP？

| 优点 | 说明 |
|------|------|
| 简单可靠 | HTTP 在移动端支持完善 |
| 快速实现 | 无需复杂的 P2P 协议 |
| 调试方便 | 标准协议，工具丰富 |
| 后续可升级 | 验证需求后可引入 WireGuard |

## 架构设计

```
┌─────────────────────────────────────────────────────────────────┐
│                         Phase 4 架构                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────┐       局域网 HTTP        ┌─────────────┐     │
│   │   移动端     │ ◄─────────────────────► │   桌面端     │     │
│   │  (Client)   │                          │  (Server)   │     │
│   │             │                          │  :19528     │     │
│   └─────────────┘                          └─────────────┘     │
│          │                                        │             │
│          │                                        │             │
│          │         跨网络（可选）                  │             │
│          │              │                         │             │
│          ▼              ▼                         ▼             │
│   ┌─────────────────────────────────────────────────────┐      │
│   │                  公共 HTTP Relay                     │      │
│   │                   (Phase 4.8)                        │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   配对方式:                                                     │
│   ├── 二维码扫描 (推荐)                                         │
│   ├── 手动输入 IP                                               │
│   └── UDP 广播发现 (局域网)                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 前置条件

- Phase 1-3 桌面端 MVP 已完成
- Tauri 2 移动端开发环境配置完成

---

## 核心任务

### 4.1 桌面端 HTTP Server

**目标**：在桌面端启动 HTTP 服务，供移动端连接

**技术选型**：axum（轻量、async、Rust 生态）

**实现步骤**：

1. 添加依赖到 `Cargo.toml`

```toml
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors"] }
tokio = { version = "1", features = ["full"] }
```

2. 创建 `src-tauri/src/http_bridge/mod.rs` 模块

3. 实现 HTTP Server

```rust
use axum::{Router, routing::{get, post}, Json};

pub struct HttpBridge {
    port: u16,
    sessions: Arc<RwLock<HashMap<String, HttpSession>>>,
}

impl HttpBridge {
    pub async fn start(&self) -> Result<(), Error> {
        let app = Router::new()
            .route("/api/info", get(Self::get_info))
            .route("/api/sessions", post(Self::create_session))
            .route("/api/sessions/:id/files", get(Self::list_files))
            .route("/api/sessions/:id/files/:file_id", get(Self::download_file))
            .route("/api/sessions/:id/files", post(Self::upload_file))
            .layer(CorsLayer::permissive());

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        axum::serve(TcpListener::bind(addr).await?, app).await?;
        Ok(())
    }
}
```

4. 集成到 Tauri App 生命周期

**Tauri 命令**：

```rust
#[tauri::command]
async fn start_http_bridge() -> Result<String, String>;

#[tauri::command]
fn stop_http_bridge() -> Result<(), String>;

#[tauri::command]
fn get_http_bridge_url() -> Option<String>;
```

**验收标准**：

- [ ] HTTP Server 能在指定端口启动
- [ ] 能响应基本的 API 请求
- [ ] 支持 CORS（移动端浏览器需要）

---

### 4.2 HTTP API 设计

**目标**：定义桌面端和移动端之间的通信协议

**API 端点**：

```yaml
# 设备信息
GET /api/info
Response:
  device_name: string
  device_id: string
  version: string

# 创建会话（移动端扫码后调用）
POST /api/sessions
Request:
  session_id: string      # 从二维码获取
  device_name: string     # 移动端设备名
Response:
  accepted: boolean
  encryption_key: string  # base64

# 获取待传输文件列表
GET /api/sessions/:id/files
Response:
  files:
    - id: string
      name: string
      size: number
      mime_type: string

# 下载文件（支持分块）
GET /api/sessions/:id/files/:file_id
Headers:
  Range: bytes=0-65535    # 分块下载
Response:
  Content-Type: application/octet-stream
  Content-Range: bytes 0-65535/1048576
  Body: <binary data>

# 上传文件（移动端发送到桌面端）
POST /api/sessions/:id/files
Headers:
  Content-Type: multipart/form-data
  X-File-Name: photo.jpg
  X-File-Size: 1048576
Body:
  <binary data>

# WebSocket 实时通信
WS /api/sessions/:id/ws
Messages:
  - type: "progress", file_id, transferred, total
  - type: "complete", file_id
  - type: "error", message
  - type: "cancel"
```

**数据模型**：

```rust
#[derive(Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_name: String,
    pub device_id: String,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct HttpSession {
    pub id: String,
    pub peer_name: String,
    pub created_at: i64,
    pub direction: Direction,
    pub files: Vec<FileInfo>,
    pub encryption_key: Option<[u8; 32]>,
}

#[derive(Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
}
```

**验收标准**：

- [ ] API 文档完整
- [ ] 所有端点实现并测试
- [ ] 错误响应格式统一

---

### 4.3 移动端项目搭建

**目标**：配置 Tauri 2 移动端开发环境

**实现步骤**：

1. 配置 Android 开发环境

```bash
# 安装 Android Studio 和 SDK
# 配置 ANDROID_HOME 环境变量

# 添加 Rust Android 目标
rustup target add aarch64-linux-android armv7-linux-androideabi
```

2. 配置 iOS 开发环境（macOS）

```bash
# 安装 Xcode
# 添加 Rust iOS 目标
rustup target add aarch64-apple-ios x86_64-apple-ios
```

3. 初始化 Tauri 移动端

```bash
pnpm tauri android init
pnpm tauri ios init
```

4. 配置 `tauri.conf.json` 移动端特定设置

```json
{
  "bundle": {
    "iOS": {
      "minimumSystemVersion": "15.0"
    },
    "android": {
      "minSdkVersion": 26
    }
  }
}
```

**验收标准**：

- [ ] Android 模拟器能运行 App
- [ ] iOS 模拟器能运行 App（macOS）
- [ ] 基本 UI 渲染正常

---

### 4.4 二维码配对

**目标**：桌面端显示二维码，移动端扫码配对

**二维码内容**：

```json
{
  "type": "swarmdrop",
  "version": 1,
  "ip": "192.168.1.100",
  "port": 19528,
  "session_id": "a1b2c3d4",
  "device_name": "我的电脑",
  "encryption_key": "base64encodedkey..."
}
```

**桌面端实现**：

```rust
use qrcode::{QrCode, render::svg};

#[tauri::command]
fn generate_qr_code(session_id: &str) -> Result<String, String> {
    let local_ip = get_local_ip()?;
    let payload = QrCodePayload {
        r#type: "swarmdrop".to_string(),
        version: 1,
        ip: local_ip,
        port: 19528,
        session_id: session_id.to_string(),
        device_name: get_device_name(),
        encryption_key: generate_session_key(),
    };

    let json = serde_json::to_string(&payload)?;
    let code = QrCode::new(json.as_bytes())?;
    let svg = code.render::<svg::Color>().build();
    Ok(svg)
}
```

**移动端实现**：

```typescript
// 使用 Tauri 插件或 Web API 扫描二维码
import { scan } from '@anthropic/tauri-plugin-barcode-scanner';

async function scanQrCode() {
  const result = await scan();
  const payload = JSON.parse(result);

  if (payload.type !== 'swarmdrop') {
    throw new Error('Invalid QR code');
  }

  // 连接桌面端
  await connectToDesktop(payload);
}
```

**依赖**：

```toml
# 桌面端
qrcode = "0.14"

# 移动端需要扫码插件
```

**验收标准**：

- [ ] 桌面端能生成二维码
- [ ] 移动端能扫描并解析
- [ ] 扫码后能成功建立连接

---

### 4.5 局域网发现

**目标**：移动端发现同一局域网的桌面端

**方案对比**：

| 方案 | 优点 | 缺点 |
|------|------|------|
| mDNS | 标准协议 | 移动端支持差 |
| UDP 广播 | 简单可控 | 需要实现 |
| 手动输入 IP | 最可靠 | 用户体验差 |

**推荐方案**：UDP 广播 + 手动输入兜底

**UDP 广播实现**：

```rust
// 桌面端：定期广播存在
const BROADCAST_PORT: u16 = 19529;
const BROADCAST_INTERVAL: Duration = Duration::from_secs(2);

async fn broadcast_presence(device_info: &DeviceInfo) {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    let message = serde_json::to_vec(device_info)?;
    let broadcast_addr = "255.255.255.255:19529";

    loop {
        socket.send_to(&message, broadcast_addr).await?;
        tokio::time::sleep(BROADCAST_INTERVAL).await;
    }
}
```

```typescript
// 移动端：监听广播
async function discoverDesktopDevices(): Promise<DeviceInfo[]> {
  // 使用 Tauri 插件监听 UDP 广播
  const devices = await invoke('discover_devices', { timeout: 5000 });
  return devices;
}
```

**验收标准**：

- [ ] 桌面端能广播存在信息
- [ ] 移动端能发现桌面端
- [ ] 支持手动输入 IP 地址

---

### 4.6 文件传输

**目标**：实现移动端与桌面端之间的文件传输

**下载流程（移动端接收）**：

```typescript
async function downloadFile(
  baseUrl: string,
  sessionId: string,
  fileId: string,
  savePath: string,
  onProgress: (progress: number) => void
) {
  const CHUNK_SIZE = 64 * 1024; // 64KB
  const fileInfo = await fetch(`${baseUrl}/api/sessions/${sessionId}/files/${fileId}`);
  const totalSize = fileInfo.size;

  let downloaded = 0;
  const chunks: ArrayBuffer[] = [];

  while (downloaded < totalSize) {
    const end = Math.min(downloaded + CHUNK_SIZE - 1, totalSize - 1);
    const response = await fetch(
      `${baseUrl}/api/sessions/${sessionId}/files/${fileId}`,
      {
        headers: { Range: `bytes=${downloaded}-${end}` }
      }
    );
    const chunk = await response.arrayBuffer();
    chunks.push(chunk);
    downloaded += chunk.byteLength;
    onProgress(downloaded / totalSize);
  }

  // 保存文件
  await writeFile(savePath, concatenateChunks(chunks));
}
```

**上传流程（移动端发送）**：

```typescript
async function uploadFile(
  baseUrl: string,
  sessionId: string,
  filePath: string,
  onProgress: (progress: number) => void
) {
  const fileData = await readFile(filePath);
  const fileName = getFileName(filePath);

  const response = await fetch(
    `${baseUrl}/api/sessions/${sessionId}/files`,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/octet-stream',
        'X-File-Name': encodeURIComponent(fileName),
        'X-File-Size': fileData.byteLength.toString(),
      },
      body: fileData,
    }
  );

  return response.json();
}
```

**加密（可选）**：

```typescript
// 使用 Web Crypto API
async function encryptChunk(key: CryptoKey, data: ArrayBuffer): Promise<ArrayBuffer> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    data
  );
  // 返回 iv + encrypted
  return concatenate(iv, encrypted);
}
```

**验收标准**：

- [ ] 移动端能下载桌面端文件
- [ ] 移动端能上传文件到桌面端
- [ ] 大文件传输稳定（> 100MB）
- [ ] 进度显示准确

---

### 4.7 移动端 UI

**目标**：实现移动端的发送/接收界面

**页面结构**：

```
移动端 App
├── 首页
│   ├── 扫码连接按钮
│   ├── 附近设备列表
│   └── 手动输入 IP
├── 扫码页面
│   └── 相机预览 + 扫码框
├── 连接确认页面
│   ├── 设备信息
│   └── 文件列表预览
├── 传输页面
│   ├── 进度条
│   ├── 文件列表（带状态）
│   └── 取消按钮
└── 完成页面
    ├── 传输统计
    └── 打开文件按钮
```

**组件设计**：

```tsx
// 设备发现列表
interface NearbyDeviceCardProps {
  device: DeviceInfo;
  onConnect: () => void;
}

// 文件列表
interface FileListProps {
  files: FileInfo[];
  direction: 'send' | 'receive';
}

// 传输进度
interface TransferProgressProps {
  currentFile: string;
  fileProgress: number;
  totalProgress: number;
  speed: number;
  onCancel: () => void;
}
```

**适配**：

- 使用响应式布局适配不同屏幕
- 支持深色/浅色模式
- 触摸友好的交互设计

**验收标准**：

- [ ] UI 在 iOS/Android 上正常显示
- [ ] 扫码功能正常
- [ ] 传输进度实时更新
- [ ] 支持深色模式

---

### 4.8 公共 Relay（P1）

**目标**：支持跨网络传输（移动端不在同一局域网）

**方案**：部署简单的 HTTP 中继服务

```
移动端 ──HTTP──► Relay Server ◄──HTTP── 桌面端

流程:
1. 桌面端上传文件到 Relay（加密）
2. 生成下载链接
3. 分享链接给移动端
4. 移动端从 Relay 下载（解密）
```

**Relay 服务设计**：

```rust
// 简单的文件中继服务
struct RelayServer {
    storage: TempStorage,  // 临时文件存储
    max_file_size: u64,    // 最大文件大小限制
    expiry: Duration,      // 文件过期时间
}

// API
POST /upload              // 上传文件，返回下载 token
GET  /download/:token     // 下载文件
DELETE /files/:token      // 删除文件
```

**安全考虑**：

- 文件端到端加密，Relay 无法解密
- 下载 token 一次性使用
- 文件自动过期删除（如 1 小时）
- 限制单文件大小（如 1GB）

**验收标准**：

- [ ] Relay 服务可部署
- [ ] 跨网络传输成功
- [ ] 文件过期自动清理

---

## 项目结构

```
src-tauri/src/
├── http_bridge/
│   ├── mod.rs              # 模块导出
│   ├── server.rs           # HTTP Server 实现
│   ├── api.rs              # API 路由和处理器
│   ├── session.rs          # HTTP 会话管理
│   ├── qrcode.rs           # 二维码生成
│   └── discovery.rs        # UDP 广播发现
└── ...

src/
├── components/
│   ├── QRCodeDisplay.tsx   # 二维码显示（桌面端）
│   ├── QRCodeScanner.tsx   # 二维码扫描（移动端）
│   ├── NearbyDevices.tsx   # 附近设备列表
│   ├── ManualConnect.tsx   # 手动输入 IP
│   └── MobileTransfer.tsx  # 移动端传输界面
├── hooks/
│   ├── useQRCode.ts
│   ├── useDiscovery.ts
│   └── useHttpTransfer.ts
└── pages/
    ├── mobile/
    │   ├── Home.tsx
    │   ├── Scan.tsx
    │   ├── Transfer.tsx
    │   └── Complete.tsx
    └── ...
```

---

## 测试计划

### 设备矩阵

| 桌面端 | 移动端 | 测试项 |
|--------|--------|--------|
| Windows | Android | 局域网传输 |
| Windows | iOS | 局域网传输 |
| macOS | Android | 局域网传输 |
| macOS | iOS | 局域网传输 |
| Linux | Android | 局域网传输 |

### 测试场景

1. 二维码扫码配对
2. 手动输入 IP 配对
3. 小文件传输（< 1MB）
4. 大文件传输（> 100MB）
5. 传输中断恢复
6. 多文件传输
7. 网络切换（WiFi → 移动数据）

### 性能指标

| 指标 | 目标 |
|------|------|
| 局域网传输速度 | ≥ 20 MB/s |
| 连接建立时间 | < 3 秒 |
| 二维码识别时间 | < 1 秒 |

---

## 风险与对策

| 风险 | 概率 | 对策 |
|------|------|------|
| 移动端扫码权限被拒 | 中 | 提供手动输入备选方案 |
| WiFi 隔离导致无法连接 | 低 | 提示用户检查网络设置 |
| 大文件传输超时 | 中 | 分块传输 + 断点续传 |
| iOS 后台传输中断 | 高 | 提示用户保持 App 前台 |

---

## 阶段产出

- [ ] 桌面端 HTTP Server
- [ ] 移动端 Tauri 项目
- [ ] 二维码配对功能
- [ ] 局域网设备发现
- [ ] 文件传输（上传/下载）
- [ ] 移动端 UI
- [ ] （P1）公共 Relay 服务

## 验收标准

- [ ] iOS 设备能与 Windows/macOS 互传文件
- [ ] Android 设备能与 Windows/macOS 互传文件
- [ ] 局域网传输速度 ≥ 20 MB/s
- [ ] 用户流程 < 1 分钟完成

---

## 后续优化方向（Phase 5）

如果用户反馈需要更好的移动端体验，可考虑：

1. **WireGuard (boringtun)** - 真正的 P2P，需要 VPN 权限
2. **WebRTC** - 浏览器友好，需要信令服务器
3. **移动端之间直传** - 目前需要通过桌面端中转

详见 [mobile-strategy.md](../early-research/mobile-strategy.md)
