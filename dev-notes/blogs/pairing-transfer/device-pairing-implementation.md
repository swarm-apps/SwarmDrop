# SwarmDrop 设备配对系统：基于 DHT 的去中心化配对码方案

SwarmDrop 的核心挑战之一是：两台从未通信过的设备，如何在没有中心服务器的情况下安全建立连接？本文详细介绍 SwarmDrop 设备配对系统的设计思路与实现细节——一个基于 Kademlia DHT 的去中心化配对码方案。

## 问题：去中心化世界里，如何"找到彼此"？

传统文件传输工具（如微信文件传输助手、AirDrop）依赖中心服务器或同一局域网来建立连接。SwarmDrop 需要同时支持两种场景：

```mermaid
graph TB
    subgraph "场景 1：局域网"
        A1[设备 A] <-->|mDNS 自动发现| B1[设备 B]
    end

    subgraph "场景 2：跨网络"
        A2[设备 A<br/>北京 家庭网络] ---|NAT| C[互联网]
        C ---|NAT| B2[设备 B<br/>上海 公司网络]
        A2 -.-|"❓ 如何找到对方"| B2
    end
```

局域网场景通过 mDNS 协议自动发现，问题不大。但跨网络场景下，两台设备隔着 NAT，互相不知道对方的存在。我们需要一种"不依赖中心服务器"的方式让它们建立联系。

**解决方案：6 位配对码 + Kademlia DHT。**

## 整体架构

配对系统由以下模块组成：

```mermaid
graph TB
    subgraph "前端 (React + TypeScript)"
        UI[配对 UI]
        TS[pairing.ts<br/>命令封装]
        NET[network.ts<br/>事件监听]
    end

    subgraph "Tauri IPC"
        CMD[commands/pairing.rs<br/>Tauri 命令层]
    end

    subgraph "后端 (Rust)"
        NM[NetManager<br/>网络管理器]
        PM[PairingManager<br/>配对管理器]
        CODE[code.rs<br/>配对码生成]
        PROTO[protocol.rs<br/>协议定义]
    end

    subgraph "P2P 网络"
        DHT[(Kademlia DHT)]
        RR[Request-Response<br/>协议]
    end

    UI --> TS
    UI --> NET
    TS --> CMD
    CMD --> NM
    NM --> PM
    PM --> CODE
    PM --> PROTO
    PM --> DHT
    PM --> RR
```

## 核心流程：配对码如何工作

整个配对过程分为两个角色：**分享方**（生成配对码的一方）和**接收方**（输入配对码的一方）。

### 完整时序图

```mermaid
sequenceDiagram
    participant SA as 分享方 UI
    participant SM as 分享方 PairingManager
    participant DHT as Kademlia DHT
    participant RM as 接收方 PairingManager
    participant RA as 接收方 UI

    Note over SA, RA: 阶段 1：生成并发布配对码

    SA->>SM: generatePairingCode(300)
    SM->>SM: 生成 6 位随机数字码
    SM->>SM: SHA256(code) → DHT Key
    SM->>DHT: put_record(key, ShareCodeRecord)
    SM->>SM: active_codes.insert(code)
    SM-->>SA: PairingCodeInfo { code: "482916" }
    SA->>SA: 显示配对码给用户

    Note over SA, RA: 阶段 2：查询设备信息

    RA->>RA: 用户输入配对码 "482916"
    RA->>RM: getDeviceInfo("482916")
    RM->>RM: SHA256("482916") → DHT Key
    RM->>DHT: get_record(key)
    DHT-->>RM: Record { publisher, value }
    RM->>RM: 解析 ShareCodeRecord
    RM-->>RA: DeviceInfo { peerId, hostname, os... }
    RA->>RA: 显示对端设备信息

    Note over SA, RA: 阶段 3：发起配对请求

    RA->>RM: requestPairing(peerId, Code)
    RM->>SM: send_request(PairingRequest)
    SM-->>SA: InboundRequest 事件
    SA->>SA: 弹出确认对话框

    Note over SA, RA: 阶段 4：确认并完成配对

    SA->>SM: respondPairingRequest(accept)
    SM->>SM: active_codes.remove(code) ✓
    SM-->>RM: PairingResponse::Success
    RM-->>RA: 配对成功
```

### 阶段详解

#### 阶段 1：生成配对码

分享方调用 `generate_pairing_code` 生成一个 6 位纯数字配对码，并将设备信息发布到 DHT。

```rust
// pairing/code.rs
const CHARSET: &[u8] = b"0123456789";
const CODE_LENGTH: usize = 6;

impl PairingCodeInfo {
    pub fn generate(expires_in_secs: u64) -> Self {
        let mut rng = rand::rng();
        let code: String = (0..CODE_LENGTH)
            .map(|_| *CHARSET.choose(&mut rng).unwrap() as char)
            .collect();
        let now = chrono::Utc::now().timestamp();
        Self {
            code,
            created_at: now,
            expires_at: now + expires_in_secs as i64,
        }
    }
}
```

**关键设计：配对码不直接作为 DHT Key。** 原始配对码经过 SHA-256 哈希（带命名空间前缀）后才作为 DHT 的 key 存储，这样即使有人扫描 DHT，也无法反推出配对码：

```mermaid
flowchart LR
    A["配对码<br/>&quot;482916&quot;"] -->|SHA-256| B["DHT Key<br/>SHA256('/swarmdrop/share-code/' + '482916')"]
    B -->|存储| C["DHT Record<br/>设备信息 + 地址 + PeerId"]
```

```rust
// src-tauri/src/pairing/dht_key.rs
const NS_SHARE_CODE: &[u8] = b"/swarmdrop/share-code/";
const NS_ONLINE: &[u8] = b"/swarmdrop/online/";

fn dht_key(namespace: &[u8], id: &[u8]) -> RecordKey {
    sha2::Sha256::digest([namespace, id].concat()).to_vec().into()
}

pub fn share_code_key(code: &str) -> RecordKey {
    dht_key(NS_SHARE_CODE, code.as_bytes())
}

pub fn online_key(peer_id_bytes: &[u8]) -> RecordKey {
    dht_key(NS_ONLINE, peer_id_bytes)
}
```

命名空间前缀确保不同用途的 DHT Key 不会冲突——即使配对码和某个 PeerId 的字节恰好相同，它们的哈希结果也不同。

发布到 DHT 的值是 `ShareCodeRecord`，包含分享方的设备信息和可达地址：

```rust
// pairing/code.rs
pub struct ShareCodeRecord {
    pub os_info: OsInfo,         // hostname, os, platform, arch (flatten)
    pub created_at: i64,
    pub expires_at: i64,
    pub listen_addrs: Vec<Multiaddr>,  // 发布者的可达地址
}
```

同时，配对码也保存在内存中的 `active_codes`（`DashMap`），用于后续验证：

```rust
// pairing/manager.rs
pub async fn generate_code(&self, expires_in_secs: u64) -> AppResult<PairingCodeInfo> {
    let code_info = PairingCodeInfo::generate(expires_in_secs);

    // 获取本节点可达地址（listeners + external + relay circuit）
    let addrs = self.client.get_addrs().await?;
    let mut share_record = ShareCodeRecord::from(&code_info);
    share_record.listen_addrs = addrs;

    // 发布到 DHT（使用命名空间 Key）
    self.client.put_record(Record {
        key: dht_key::share_code_key(&code_info.code),
        value: serde_json::to_vec(&share_record)?,
        publisher: Some(self.peer_id),
        expires: None,
    }).await?;

    // 保存到本地，用于后续验证
    self.active_codes.insert(code_info.code.clone(), code_info.clone());

    Ok(code_info)
}
```

#### 阶段 2：查询设备信息

接收方输入配对码后，同样对配对码做 SHA-256 哈希，从 DHT 中查询对应的记录：

```rust
pub async fn get_device_info(&self, code: &str) -> AppResult<(PeerId, ShareCodeRecord)> {
    let key = dht_key::share_code_key(code);
    let record = self.client.get_record(key).await?.record;

    // 从 record.publisher 获取分享方的 PeerId
    let peer_id = record.publisher.ok_or(AppError::Peer("无效的配对码".into()))?;
    let share_record: ShareCodeRecord = serde_json::from_slice(&record.value)?;

    // 检查是否过期
    if share_record.expires_at < chrono::Utc::now().timestamp() {
        return Err(AppError::Peer("配对码已过期".into()));
    }

    Ok((peer_id, share_record))
}
```

这一步得到三个关键信息：
- **PeerId** — 分享方在 P2P 网络中的唯一标识，用于建立直接连接
- **ShareCodeRecord** — 分享方的设备信息（设备名、操作系统等），用于在 UI 上展示
- **listen_addrs** — 分享方的可达地址（含 relay circuit 地址），用于跨网络场景下注册到 Swarm 地址簿后直接 dial

#### 阶段 3：发起配对请求

获得 PeerId 后，接收方通过 libp2p 的 Request-Response 协议直接向分享方发送配对请求：

```rust
pub async fn request_pairing(
    &self,
    peer_id: PeerId,
    method: PairingMethod,
    addrs: Option<Vec<Multiaddr>>,
) -> AppResult<PairingResponse> {
    // 注册对端地址（跨网络场景下从 ShareCodeRecord.listen_addrs 获取）
    if let Some(addrs) = addrs {
        if !addrs.is_empty() {
            self.client.add_peer_addrs(peer_id, addrs).await?;
        }
    }

    // 如果未连接则先 dial
    if !self.client.is_connected(peer_id).await? {
        self.client.dial(peer_id).await?;
    }

    let res = self.client.send_request(
        peer_id,
        AppRequest::Pairing(PairingRequest {
            os_info: OsInfo::default(),
            method,
            timestamp: chrono::Utc::now().timestamp(),
        }),
    ).await?;

    match res {
        AppResponse::Pairing(pairing_res) => Ok(pairing_res),
    }
}
```

关键改进：`request_pairing` 接受可选的 `addrs` 参数。在跨网络场景下，前端从 `get_device_info` 返回的 `ShareCodeRecord.listen_addrs` 中拿到对端地址，传给 `request_pairing`，后端通过 `add_peer_addrs` 将地址注册到 Swarm 地址簿后再 dial，确保 NAT 后的节点也能通过 relay circuit 地址建立连接。

分享方收到请求后，前端会触发 `inboundRequest` 事件，弹出确认对话框：

```typescript
// 前端监听事件
start((event) => {
  if (event.type === "inboundRequest") {
    // event.peerId — 请求方 PeerId
    // event.pendingId — 请求标识，用于响应
    // event.request — PairingRequest 内容
    showPairingDialog(event);
  }
});
```

#### 阶段 4：确认并完成配对

分享方用户确认后，后端验证配对码并返回响应：

```rust
pub async fn handle_pairing_request(
    &self,
    pending_id: u64,
    method: &PairingMethod,
    response: PairingResponse,
) -> AppResult<()> {
    // Code 模式：验证并消耗配对码
    if let PairingMethod::Code { code } = method {
        let (_, info) = self.active_codes
            .remove(code.as_str())
            .ok_or(AppError::InvalidCode)?;

        if info.is_expired() {
            return Err(AppError::ExpiredCode);
        }
    }
    // Direct 模式（局域网）：无需配对码，用户确认即可

    self.client
        .send_response(pending_id, AppResponse::Pairing(response))
        .await?;
    Ok(())
}
```

注意 `remove` 而不是 `get`：**配对码是一次性的**，使用后立即从 `active_codes` 中删除，防止重放。

## 两种配对模式

系统支持两种配对方式，通过 `PairingMethod` 枚举区分：

```mermaid
graph TB
    PM{PairingMethod}
    PM -->|跨网络| CODE["Code<br/>配对码模式"]
    PM -->|局域网| DIRECT["Direct<br/>直连模式"]

    CODE --> C1["需要 6 位配对码"]
    CODE --> C2["通过 DHT 发现对端"]
    CODE --> C3["后端验证码有效性"]

    DIRECT --> D1["无需配对码"]
    DIRECT --> D2["通过 mDNS 发现对端"]
    DIRECT --> D3["用户 UI 确认即可"]
```

```rust
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PairingMethod {
    Code { code: String },  // 跨网络：需要配对码
    Direct,                  // 局域网：直接发现
}
```

前端对应的 TypeScript 类型：

```typescript
export type PairingMethod =
  | { type: "code"; code: string }
  | { type: "direct" };
```

## 协议设计

所有 P2P 通信都通过统一的请求/响应协议：

```mermaid
graph LR
    subgraph "请求 (AppRequest)"
        AR["{ type: &quot;pairing&quot;, ... }"]
    end

    subgraph "响应 (AppResponse)"
        AP["{ type: &quot;pairing&quot;, ... }"]
    end

    AR -->|Request-Response| AP
```

```rust
// 应用层请求 — 未来可扩展文件传输等
pub enum AppRequest {
    Pairing(PairingRequest),
    // Transfer(TransferRequest),  // 未来
}

// 应用层响应
pub enum AppResponse {
    Pairing(PairingResponse),
    // Transfer(TransferResponse),  // 未来
}

// 统一的网络客户端类型
pub type AppNetClient = NetClient<AppRequest, AppResponse>;
```

序列化使用 serde 的 **internally tagged** 策略，JSON 格式如下：

```json
// PairingMethod::Code
{ "type": "code", "code": "482916" }

// PairingMethod::Direct
{ "type": "direct" }

// PairingResponse::Success
{ "status": "success" }

// PairingResponse::Refused
{ "status": "refused", "reason": "用户拒绝" }
```

## 网络管理架构

所有网络功能通过 `NetManager` 统一管理，采用分层架构：

```mermaid
graph TB
    subgraph "Tauri State"
        NMS["NetManagerState<br/>Mutex&lt;Option&lt;NetManager&gt;&gt;"]
    end

    subgraph "NetManager"
        CLIENT["AppNetClient<br/>网络客户端"]
        PID["PeerId<br/>节点标识"]
        PM["PairingManager<br/>配对管理器"]
    end

    subgraph "未来扩展"
        TM["TransferManager<br/>文件传输管理器"]
        SM["SyncManager<br/>同步管理器"]
    end

    NMS --> CLIENT
    NMS --> PID
    NMS --> PM
    NMS -.-> TM
    NMS -.-> SM

    PM --> CLIENT
    TM -.-> CLIENT
    SM -.-> CLIENT
```

```rust
pub struct NetManager {
    pub client: AppNetClient,
    pub peer_id: PeerId,
    pub pairing: PairingManager,
    // pub transfer: TransferManager,  // 未来
}
```

生命周期管理很清晰——启动时宣布上线（含可达地址），关闭前宣布离线：

```rust
// 启动
let net_manager = NetManager::new(client, peer_id);
net_manager.pairing().announce_online().await?;  // DHT put_record(online_key, OnlineRecord)
app.manage(Mutex::new(Some(net_manager)));

// 关闭
let manager = guard.as_ref().unwrap();
manager.pairing().announce_offline().await?;  // DHT remove_record(online_key)
guard.take();  // drop 释放所有资源
```

`announce_online` 使用 `put_record` 而非 `start_provide`，因为 Provider Record 只存 PeerId 不携带地址，NAT 后的节点无法被发现。`OnlineRecord` 包含完整的可达地址（listeners + external + relay circuit），对端可直接注册地址并 dial。

## Tauri 命令层：前后端桥梁

Tauri 命令层是一层薄封装，负责从 `NetManagerState` 中获取 `PairingManager` 并调用其方法：

```mermaid
flowchart LR
    subgraph "前端 TypeScript"
        F1["generatePairingCode()"]
        F2["getDeviceInfo()"]
        F3["requestPairing()"]
        F4["respondPairingRequest()"]
    end

    subgraph "Tauri IPC"
        direction TB
        C1["generate_pairing_code"]
        C2["get_device_info"]
        C3["request_pairing"]
        C4["respond_pairing_request"]
    end

    subgraph "PairingManager"
        M1["generate_code()"]
        M2["get_device_info()"]
        M3["request_pairing()"]
        M4["handle_pairing_request()"]
    end

    F1 -->|invoke| C1 --> M1
    F2 -->|invoke| C2 --> M2
    F3 -->|invoke| C3 --> M3
    F4 -->|invoke| C4 --> M4
```

每个 Tauri 命令的模式一致：

```rust
#[tauri::command]
pub async fn generate_pairing_code(
    net: State<'_, NetManagerState>,
    expires_in_secs: Option<u64>,
) -> AppResult<PairingCodeInfo> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or_else(not_started)?;
    manager.pairing()
        .generate_code(expires_in_secs.unwrap_or(300))
        .await
}
```

前端封装同样简洁：

```typescript
export async function generatePairingCode(
  expiresInSecs?: number,
): Promise<PairingCodeInfo> {
  return invoke<PairingCodeInfo>("generate_pairing_code", { expiresInSecs });
}
```

## 数据流全景

从前端到 P2P 网络的完整数据流：

```mermaid
flowchart TB
    subgraph "React 前端"
        UI["配对页面"]
    end

    subgraph "TypeScript 命令层"
        TS["pairing.ts"]
    end

    subgraph "Tauri IPC 边界"
        IPC["invoke / Channel"]
    end

    subgraph "Rust 命令层"
        CMD["commands/pairing.rs"]
    end

    subgraph "业务逻辑层"
        PM["PairingManager"]
        CODE["PairingCodeInfo"]
        RECORD["ShareCodeRecord"]
    end

    subgraph "P2P 网络层 (swarm-p2p-core)"
        CLIENT["NetClient"]
        KAD["Kademlia DHT"]
        RR["Request-Response"]
        MDNS["mDNS"]
    end

    subgraph "传输层"
        TCP["TCP / QUIC"]
        RELAY["Relay 中继"]
        DCUTR["DCUtR 打洞"]
    end

    UI -->|用户操作| TS
    TS -->|invoke| IPC
    IPC -->|反序列化| CMD
    CMD -->|业务调用| PM
    PM --> CODE
    PM --> RECORD
    PM -->|DHT 操作| CLIENT
    PM -->|P2P 消息| CLIENT
    CLIENT --> KAD
    CLIENT --> RR
    CLIENT --> MDNS
    KAD --> TCP
    RR --> TCP
    RR --> RELAY
    RELAY --> DCUTR
```

## 安全考量

### 配对码哈希

配对码经 SHA-256 哈希（带命名空间前缀 `/swarmdrop/share-code/`）后存入 DHT，即使攻击者能遍历 DHT 中的所有 key，也无法从 hash 反推出配对码。命名空间前缀还确保配对码 Key 与在线宣告 Key 不会冲突。

### 配对码一次性消耗

配对码在成功配对后立即从 `active_codes` 中删除（`remove` 而非 `get`），防止重放攻击。

### 配对码过期机制

```mermaid
graph LR
    A["生成配对码"] -->|5 分钟有效期| B["DHT Record 过期"]
    A -->|本地检查| C["is_expired()"]
    B --> D["自动清理"]
    C -->|过期| E["返回 ExpiredCode 错误"]
```

双重过期检查：
1. **DHT 层面**：`Record` 自带 `expires` 字段，DHT 会自动清理过期记录
2. **应用层面**：`PairingCodeInfo::is_expired()` 在验证时再次检查时间戳

### 局域网直连验证

Direct 模式不需要配对码，但要求用户在 UI 上手动确认对端设备信息。这种模式仅在 mDNS 发现的局域网设备之间使用，攻击面有限。

## 模块组织

```
src-tauri/src/
├── device.rs              # OsInfo — 设备信息（共享模块）
├── protocol.rs            # P2P 协议类型定义
├── error.rs               # 错误类型 (含 ExpiredCode, InvalidCode)
├── pairing/
│   ├── mod.rs
│   ├── code.rs            # PairingCodeInfo, ShareCodeRecord, OnlineRecord
│   ├── dht_key.rs         # DHT Key 命名空间 (share_code_key, online_key)
│   └── manager.rs         # PairingManager 核心业务逻辑
├── commands/
│   ├── mod.rs             # NetManager、start/shutdown 命令
│   └── pairing.rs         # 配对相关 Tauri 命令
└── lib.rs                 # 模块声明 & 命令注册

src/commands/
├── network.ts             # 网络命令 & 事件类型
├── pairing.ts             # 配对命令 & 类型定义
└── index.ts               # 统一导出
```

`OsInfo` 被提取为独立的 `device.rs` 模块，因为它同时被 `pairing` 和 `commands` 两个模块使用：

```mermaid
graph TB
    DEVICE["device.rs<br/>OsInfo"]

    CMD["commands/mod.rs<br/>agent_version 构建"]
    CODE["pairing/code.rs<br/>ShareCodeRecord"]
    MGR["pairing/manager.rs<br/>PairingRequest"]
    PROTO["protocol.rs<br/>PairingRequest"]

    DEVICE --> CMD
    DEVICE --> CODE
    DEVICE --> MGR
    DEVICE --> PROTO
```

## 总结

SwarmDrop 的配对系统通过"配对码 + DHT"实现了去中心化的设备发现与连接建立：

| 组件 | 职责 |
|------|------|
| `PairingCodeInfo` | 生成 6 位随机码 |
| `dht_key` | DHT Key 命名空间管理（share_code_key, online_key） |
| `ShareCodeRecord` | 存入 DHT 的设备信息 + 可达地址 |
| `OnlineRecord` | 在线宣告记录，含可达地址供已配对设备发现 |
| `PairingManager` | 核心业务逻辑：生成、查询、请求、响应、在线宣告 |
| `NetManager` | 统一管理网络客户端和子模块 |
| `protocol.rs` | 定义 P2P 通信协议，支持未来扩展 |

这套方案的优势在于：

1. **完全去中心化** — 不依赖任何中心服务器，配对码通过 DHT 分布式存储
2. **用户友好** — 只需输入 6 位数字码，比交换 PeerId 简单得多
3. **安全** — 配对码哈希存储、一次性消耗、自动过期
4. **可扩展** — `AppRequest` / `AppResponse` 枚举和 `NetManager` 架构为文件传输等后续功能预留了空间
5. **双模式** — Code 模式用于跨网络，Direct 模式用于局域网，覆盖全场景
