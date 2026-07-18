# Phase 3: 文件传输

> **范围**: 仅桌面端 (Windows / macOS / Linux)
>
> **参考**: [implementation-roadmap.md](implementation-roadmap.md) 查看整体规划

## 目标

实现端到端加密的文件传输功能，包括文件分块、进度追踪、断点续传和错误恢复。

**注意**：本阶段完成后即为**桌面端 MVP**。移动端文件传输将在 Phase 4 使用 HTTP 方案实现。

## 前置条件

- Phase 1 网络连接已完成
- Phase 2 设备配对已完成
- 已建立配对连接

## 核心任务

### 3.1 Request-Response 协议

**目标**：基于 libp2p request-response 实现文件传输协议

**协议设计**：
```rust
// 协议标识
const PROTOCOL_NAME: &str = "/swarmdrop/transfer/1.0.0";

// 请求类型
enum TransferRequest {
    // 请求文件列表
    FileListRequest {
        session_id: [u8; 8],
    },

    // 请求文件块
    ChunkRequest {
        session_id: [u8; 8],
        file_id: u32,
        chunk_index: u32,
    },

    // 确认接收完成
    TransferComplete {
        session_id: [u8; 8],
        file_id: u32,
        checksum: [u8; 32],
    },

    // 取消传输
    CancelTransfer {
        session_id: [u8; 8],
    },
}

// 响应类型
enum TransferResponse {
    // 文件列表
    FileList {
        files: Vec<FileInfo>,
        total_size: u64,
    },

    // 文件块数据
    Chunk {
        file_id: u32,
        chunk_index: u32,
        data: Vec<u8>,
        is_last: bool,
    },

    // 确认
    Ack {
        success: bool,
        message: Option<String>,
    },

    // 错误
    Error {
        code: u32,
        message: String,
    },
}
```

**实现步骤**：
1. 定义 `TransferCodec` 实现序列化/反序列化
2. 配置 `request_response::Behaviour`
3. 实现请求处理器
4. 实现响应处理器

**Rust 依赖**：
```toml
libp2p = { version = "0.54", features = [
    # ... 之前的 features
    "request-response",
] }
```

**验收标准**：
- [ ] 协议能正确编码/解码
- [ ] 请求能正确路由和处理
- [ ] 超时和错误能正确处理

---

### 3.2 文件分块

**目标**：将大文件分割成固定大小的块进行传输

**分块策略**：
```rust
const CHUNK_SIZE: usize = 64 * 1024;  // 64 KB per chunk
const MAX_CONCURRENT_CHUNKS: usize = 4;  // 最大并发请求数

struct FileChunker {
    file_path: PathBuf,
    file_size: u64,
    chunk_size: usize,
    total_chunks: u32,
}

impl FileChunker {
    fn read_chunk(&self, index: u32) -> Result<Vec<u8>, Error>;
    fn total_chunks(&self) -> u32;
}
```

**数据模型**：
```rust
struct FileInfo {
    id: u32,                    // 文件 ID（会话内唯一）
    name: String,               // 文件名
    path: String,               // 相对路径（用于文件夹）
    size: u64,                  // 文件大小
    mime_type: Option<String>,  // MIME 类型
    checksum: [u8; 32],         // SHA256 校验和
    total_chunks: u32,          // 总块数
}

struct ChunkInfo {
    file_id: u32,
    index: u32,
    offset: u64,
    size: usize,
    checksum: [u8; 32],
}
```

**实现步骤**：
1. 创建 `src-tauri/src/transfer/chunker.rs`
2. 实现文件分块读取
3. 实现块校验和计算
4. 实现并发块请求管理

**验收标准**：
- [ ] 文件能正确分块
- [ ] 块校验和能验证完整性
- [ ] 并发请求数可控

---

### 3.3 端到端加密

**目标**：使用 XChaCha20-Poly1305 加密传输内容

**加密方案**：
```rust
use chacha20poly1305::{XChaCha20Poly1305, Key, XNonce, aead::Aead};

struct TransferEncryption {
    cipher: XChaCha20Poly1305,
    nonce_counter: u64,
}

impl TransferEncryption {
    fn new(key: &[u8; 32]) -> Self;

    // 加密块
    fn encrypt_chunk(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error>;

    // 解密块
    fn decrypt_chunk(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, Error>;
}
```

**密钥管理**：
```
1. 配对时生成随机 256-bit 密钥
2. 密钥通过分享码或握手协议传递
3. 每个块使用递增的 nonce
4. 传输完成后密钥销毁
```

**Rust 依赖**：
```toml
chacha20poly1305 = "0.10"
rand = "0.8"
```

**实现步骤**：
1. 创建 `src-tauri/src/transfer/crypto.rs`
2. 实现密钥生成
3. 实现加密/解密函数
4. 集成到传输流程

**验收标准**：
- [ ] 加密/解密正确
- [ ] nonce 不重复
- [ ] 密钥安全存储和销毁

---

### 3.4 发送端实现

**目标**：实现文件发送流程

**发送流程**：
```
1. 用户选择文件
2. 生成分享码，等待接收方
3. 接收方连接
4. 发送文件列表
5. 接收方确认接收
6. 逐块发送文件（加密）
7. 等待传输完成确认
8. 清理临时资源
```

**状态机**：
```rust
enum SendState {
    Preparing,          // 准备中
    WaitingForReceiver, // 等待接收方
    Connected,          // 已连接
    Negotiating,        // 协商中
    Transferring {      // 传输中
        current_file: u32,
        current_chunk: u32,
        bytes_sent: u64,
    },
    Completing,         // 完成中
    Completed,          // 已完成
    Failed(String),     // 失败
    Cancelled,          // 已取消
}
```

**Tauri 命令**：
```rust
#[tauri::command]
async fn prepare_send(file_paths: Vec<String>) -> Result<SendSession, String>;

#[tauri::command]
async fn start_send(session_id: String) -> Result<(), String>;

#[tauri::command]
fn cancel_send(session_id: String) -> Result<(), String>;
```

**前端事件**：
```typescript
listen('send-progress', (event: SendProgress) => { ... });
listen('send-completed', (event: SendCompleted) => { ... });
listen('send-failed', (event: SendFailed) => { ... });
```

**验收标准**：
- [ ] 能选择多个文件/文件夹
- [ ] 进度实时更新
- [ ] 可取消发送
- [ ] 失败有明确提示

---

### 3.5 接收端实现

**目标**：实现文件接收流程

**接收流程**：
```
1. 输入分享码或选择附近设备
2. 连接发送方
3. 获取文件列表
4. 用户确认接收
5. 选择保存位置
6. 逐块接收文件（解密）
7. 校验文件完整性
8. 发送完成确认
```

**状态机**：
```rust
enum ReceiveState {
    Connecting,         // 连接中
    Connected,          // 已连接
    FileListReceived,   // 收到文件列表
    WaitingConfirm,     // 等待用户确认
    Transferring {      // 传输中
        current_file: u32,
        current_chunk: u32,
        bytes_received: u64,
    },
    Verifying,          // 校验中
    Completed,          // 已完成
    Failed(String),     // 失败
    Cancelled,          // 已取消
}
```

**Tauri 命令**：
```rust
#[tauri::command]
async fn connect_and_receive(share_code: String) -> Result<ReceiveSession, String>;

#[tauri::command]
fn confirm_receive(session_id: String, save_path: String) -> Result<(), String>;

#[tauri::command]
fn cancel_receive(session_id: String) -> Result<(), String>;
```

**前端事件**：
```typescript
listen('receive-file-list', (event: FileListEvent) => { ... });
listen('receive-progress', (event: ReceiveProgress) => { ... });
listen('receive-completed', (event: ReceiveCompleted) => { ... });
listen('receive-failed', (event: ReceiveFailed) => { ... });
```

**验收标准**：
- [ ] 能显示待接收文件列表
- [ ] 可选择保存位置
- [ ] 进度实时更新
- [ ] 文件校验通过

---

### 3.6 进度追踪

**目标**：实时显示传输进度

**进度数据**：
```rust
struct TransferProgress {
    session_id: String,
    direction: Direction,           // Send / Receive
    state: TransferState,
    total_files: u32,
    completed_files: u32,
    current_file: Option<FileProgress>,
    total_bytes: u64,
    transferred_bytes: u64,
    speed: f64,                     // bytes per second
    eta: Option<u64>,               // 预计剩余秒数
    started_at: i64,
    elapsed: u64,
}

struct FileProgress {
    file_id: u32,
    name: String,
    size: u64,
    transferred: u64,
    chunks_completed: u32,
    total_chunks: u32,
}
```

**速度计算**：
```rust
struct SpeedCalculator {
    samples: VecDeque<(Instant, u64)>,  // (时间, 字节数)
    window: Duration,                    // 滑动窗口
}

impl SpeedCalculator {
    fn add_sample(&mut self, bytes: u64);
    fn get_speed(&self) -> f64;  // bytes/sec
    fn get_eta(&self, remaining: u64) -> Option<u64>;
}
```

**实现步骤**：
1. 创建 `src-tauri/src/transfer/progress.rs`
2. 实现滑动窗口速度计算
3. 实现进度事件发送
4. 前端进度条组件

**验收标准**：
- [ ] 进度百分比准确
- [ ] 速度显示平滑
- [ ] ETA 预估合理
- [ ] 每个文件进度可见

---

### 3.7 错误处理与重试

**目标**：处理传输过程中的各种错误

**错误类型**：
```rust
enum TransferError {
    ConnectionLost,         // 连接断开
    Timeout,                // 超时
    ChecksumMismatch,       // 校验失败
    DiskFull,               // 磁盘空间不足
    PermissionDenied,       // 权限不足
    FileNotFound,           // 文件不存在
    DecryptionFailed,       // 解密失败
    ProtocolError(String),  // 协议错误
    Cancelled,              // 用户取消
}
```

**重试策略**：
```rust
struct RetryPolicy {
    max_retries: u32,           // 最大重试次数
    initial_delay: Duration,    // 初始延迟
    max_delay: Duration,        // 最大延迟
    backoff_factor: f64,        // 退避因子
}

impl RetryPolicy {
    fn should_retry(&self, error: &TransferError, attempt: u32) -> bool;
    fn get_delay(&self, attempt: u32) -> Duration;
}
```

**实现步骤**：
1. 定义错误类型枚举
2. 实现重试策略
3. 块级别的重试逻辑
4. 连接断开自动重连

**验收标准**：
- [ ] 网络抖动能自动恢复
- [ ] 校验失败能重传
- [ ] 错误信息用户友好
- [ ] 可配置重试次数

---

### 3.8 断点续传（P1）

**目标**：支持中断后恢复传输

**持久化信息**：
```rust
struct TransferCheckpoint {
    session_id: String,
    share_code: Option<String>,
    peer_id: PeerId,
    direction: Direction,
    files: Vec<FileCheckpoint>,
    encryption_key: [u8; 32],
    created_at: i64,
}

struct FileCheckpoint {
    file_id: u32,
    name: String,
    path: String,
    size: u64,
    checksum: [u8; 32],
    completed_chunks: BitVec,   // 已完成的块
    temp_path: PathBuf,         // 临时文件路径
}
```

**恢复流程**：
```
1. 检查本地是否有未完成的传输
2. 显示恢复选项
3. 用户选择恢复
4. 重新连接对方
5. 协商续传位置
6. 从断点继续传输
```

**Tauri 命令**：
```rust
#[tauri::command]
fn get_resumable_transfers() -> Vec<TransferCheckpoint>;

#[tauri::command]
async fn resume_transfer(session_id: String) -> Result<(), String>;

#[tauri::command]
fn discard_checkpoint(session_id: String) -> Result<(), String>;
```

**验收标准**：
- [ ] 断点信息正确保存
- [ ] 重启后能恢复传输
- [ ] 续传不丢失数据
- [ ] 可清除断点记录

---

### 3.9 传输 UI

**目标**：实现传输相关的前端界面

**界面组件**：

1. **文件选择器**
```tsx
interface FilePickerProps {
  onFilesSelected: (files: FileInfo[]) => void;
  multiple: boolean;
  allowFolders: boolean;
}
```

2. **文件列表预览**
```tsx
interface FileListPreviewProps {
  files: FileInfo[];
  totalSize: number;
  direction: 'send' | 'receive';
  onConfirm: () => void;
  onCancel: () => void;
}
```

3. **传输进度组件**
```tsx
interface TransferProgressProps {
  progress: TransferProgress;
  onCancel: () => void;
}
```

4. **传输完成组件**
```tsx
interface TransferCompleteProps {
  result: TransferResult;
  onOpenFolder: () => void;
  onClose: () => void;
}
```

**UI 状态流转**：
```
发送方：
选择文件 → 生成分享码 → 等待连接 → 传输中 → 完成

接收方：
输入分享码 → 连接中 → 文件列表确认 → 选择保存位置 → 传输中 → 完成
```

**验收标准**：
- [ ] 拖拽选择文件
- [ ] 文件列表清晰展示
- [ ] 进度条平滑动画
- [ ] 完成后可打开文件夹

---

## 项目结构

```
src-tauri/src/
├── transfer/
│   ├── mod.rs              # 模块导出
│   ├── protocol.rs         # Request-Response 协议
│   ├── codec.rs            # 编解码器
│   ├── chunker.rs          # 文件分块
│   ├── crypto.rs           # 加密/解密
│   ├── sender.rs           # 发送端逻辑
│   ├── receiver.rs         # 接收端逻辑
│   ├── progress.rs         # 进度追踪
│   ├── retry.rs            # 重试策略
│   └── checkpoint.rs       # 断点续传
├── identity/
│   └── ...                 # Phase 2
├── pairing/
│   └── ...                 # Phase 2
└── network/
    └── ...                 # Phase 1

src/
├── components/
│   ├── FilePicker.tsx
│   ├── FileListPreview.tsx
│   ├── TransferProgress.tsx
│   ├── TransferComplete.tsx
│   └── ...
├── hooks/
│   ├── useTransfer.ts
│   └── useFileSelect.ts
└── pages/
    ├── Send.tsx
    └── Receive.tsx
```

---

## 性能目标

| 指标 | 目标值 |
|------|--------|
| 局域网传输速度 | ≥ 50 MB/s |
| 跨网络传输速度 | ≥ 5 MB/s |
| 单文件大小支持 | 10 GB+ |
| 内存占用（传输时） | < 200 MB |
| CPU 占用（传输时） | < 30% |

---

## 测试计划

### 单元测试

- 分块算法正确性
- 加密/解密正确性
- 进度计算准确性
- 状态机转换

### 集成测试

- 小文件传输（< 1 MB）
- 大文件传输（> 1 GB）
- 多文件传输
- 文件夹传输
- 传输中断恢复

### 压力测试

- 最大文件大小
- 长时间传输稳定性
- 网络波动模拟

### 测试场景

1. 正常传输完成
2. 传输中取消
3. 网络断开恢复
4. 校验失败重传
5. 磁盘空间不足
6. 并发传输

---

## 风险与对策

| 风险 | 对策 |
|------|------|
| 大文件内存溢出 | 流式读取，固定 buffer |
| 加密性能瓶颈 | 使用硬件加速（AES-NI） |
| 网络带宽占满 | 可选限速功能 |
| 传输中断数据丢失 | 原子写入 + 断点续传 |

---

## 阶段产出

- [ ] Request-Response 传输协议
- [ ] 文件分块和重组
- [ ] XChaCha20-Poly1305 端到端加密
- [ ] 发送/接收完整流程
- [ ] 实时进度追踪
- [ ] 错误处理和重试
- [ ] 传输相关 UI 组件
- [ ] （P1）断点续传

## MVP 验收标准

- [ ] 两台局域网设备可互传 100MB 文件
- [ ] 两台跨网络设备可互传 100MB 文件
- [ ] 传输速度：局域网 ≥ 30 MB/s，跨网络 ≥ 1 MB/s
- [ ] 端到端加密，中继节点无法解密
- [ ] 端到端流程 < 1 分钟完成
