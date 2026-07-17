# Tickets —— 邀请链接 / 分享码编码

**iroh 1.0.2 · 调研 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`**

> API 用法 → `/iroh` skill。这里讲**ticket 能做什么、不能做什么、代价多少**。

## 一句话定位

**ticket 只是「postcard + base32 的自描述地址信封」，不是 rendezvous 方案，更不是一次性凭证。**

它**零过期、零 nonce、零签名**。要一次性/过期语义，必须自己在 payload 里做 + 服务端记状态。

## iroh-tickets

- **成熟度**：**production**
- **依据**：
  - version 1.0.0（`iroh-tickets/Cargo.toml:3`）；HEAD 2026-06-15 `chore: Release iroh-tickets version 1.0.0`；README 无任何 experimental/免责声明
  - 依赖面：本 study 集合内 **10 个 Cargo.toml** 依赖它，但只跨 **7 个仓**（browser-chat 贡献 2 个）。其中 4/10 是 examples/experiments（browser-chat + iroh-gateway 在 iroh-examples；h3-iroh + iroh-dag-sync 在 iroh-experiments）
  - **真正的生产采用面是 6 个 crate**：iroh-blobs / iroh-docs / dumbpipe / iroh-ffi / iroh-ffi·iroh-js / iroh-c-ffi
  - ⚠️ 本地为 shallow clone，无法据此判断提交频率
- **入口**：`iroh-tickets/src/lib.rs`
- **规模**：整个 crate 只有 2 个源文件（lib.rs 109 行 + endpoint.rs 235 行），**无网络代码、无状态、无 IO**

### 真正的价值是那个 trait，不是 EndpointTicket

`EndpointTicket` 的全部状态就是**一个字段**（`endpoint.rs:30-32`）：

```rust
pub struct EndpointTicket { addr: EndpointAddr }
```

塞不进 nonce / expires_at / 设备名。**扩展的唯一方式是自己 impl `Ticket` trait**（`lib.rs:26-65`，只要求 KIND / encode_bytes / decode_bytes，encode_string/decode_string 有默认实现）。

### 自定义 ticket 模板

照 `iroh-docs/src/ticket.rs` 的结构（那是唯一用了 verification 的实现）：

```rust
use iroh_tickets::{Ticket, ParseError};
use serde::{Serialize, Deserialize};

// 单变体 enum：强制 postcard 写出 1 字节判别符，给未来留版本位
#[derive(Serialize, Deserialize)]
enum MyInviteWire { V0(MyInvite) }

#[derive(Serialize, Deserialize, Clone, Debug, derive_more::Display)]
#[display("{}", Ticket::encode_string(self))]
pub struct MyInvite {
    pub addr: iroh::EndpointAddr,   // 直接存 EndpointAddr —— 内含完整 BTreeSet<TransportAddr>
    pub nonce: [u8; 16],            // 一次性语义要自己加，ticket 本身没有
    pub expires_at: i64,            // 过期语义要自己加，ticket 本身没有
}

impl Ticket for MyInvite {
    const KIND: &'static str = "invite";   // 必须小写 ascii（lib.rs:29）
    fn encode_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(&MyInviteWire::V0(self.clone())).expect("postcard")
    }
    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let MyInviteWire::V0(t) = postcard::from_bytes(bytes)?;   // postcard::Error -> ParseError 有 From
        if t.addr.addrs.is_empty() {
            return Err(ParseError::verification_failed("addressing info cannot be empty"));
        }
        Ok(t)
    }
}
impl std::str::FromStr for MyInvite {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Ticket::decode_string(s) }
}
// 产出: "invite" + base32(postcard(..)) 小写
```

**要点**：直接存 `EndpointAddr` 而不是照抄 BlobTicket 的拆字段写法（后者是有损的，见下）。

### 零过期 / 零一次性 / 零撤销

对 `iroh-tickets/src/` 执行 `grep -rn -i "expir|ttl|timestamp|nonce|one.time|revoke|valid_until"` → **零命中**。trait 只要求三个方法，**没有任何 validate/expire 钩子**。

**自己实现时的两层区分**：
- `decode_bytes` 里的校验只能防「格式 / 时间」
- **防不住重放** —— 重放必须服务端维护 nonce 已用集合

## ticket 长度 —— 实测

用本地 iroh-tickets 1.0.0 编译真实测试实测：

| 内容 | 字节 | 字符 |
|---|---|---|
| id-only（`EndpointAddr::from_parts(pk, [])`） | 34 | **63** |
| + 1 个真实 n0 relay（`https://use1-1.relay.n0.iroh.link./`） | 71 | **122** |
| relay + 3 个 IP（192.168 / 100.64 / IPv6） | 110 | **184** |

实例：`endpointacxfr74igmsbvsbnn73wcecg5vt3kbzncqwfrdiampuufwnhkublmaa`（63 字符）。

字节结构（由 `endpoint.rs:203-222` 的测试向量佐证）：1 字节 variant + 32 字节 endpoint id + 1 字节 addr 计数 + 每个 addr（1 字节 tag + 内容）。

**含义**：
- **人念 / 电话报码彻底不可能** —— 最短 63 字符，是 6 位码的 10 倍以上
- **二维码完全够** —— 184 字符远在 QR 容量内
- 若还要塞 nonce(16B) + expires_at(8B) + 设备名，再加约 25 字节 ≈ **+40 字符** → 按 **160~230 字符**规划预算

## base32 大小写 —— QR 优化点

- `encode_string`（lib.rs:44-49）末尾 `out.make_ascii_lowercase()` 把整串（含 base32 body）转小写
- `decode_string`（lib.rs:57-64）是 `let Some(rest) = s.strip_prefix(expected)`（**KIND 必须精确匹配 = 必须小写**）后 `BASE32_NOPAD.decode(rest.to_ascii_uppercase().as_bytes())` —— **body 解码前主动转大写，故 body 大小写不敏感**

实测验证：`endpoint` + 大写 body 解码成功；整串大写则失败，报 `wrong prefix, expected endpoint`。

**QR 优化**：QR 的 alphanumeric 模式只收大写字母+数字（每字符 5.5 bit），小写会掉进 byte 模式（8 bit/字符）。可以把 body 大写 —— 但 **KIND 前缀卡死必须小写**，会强制整串掉回 byte 模式。绕法：二维码里只编码大写 body（不含前缀），扫码后代码里补回小写前缀再 decode。

> ⚠️ 这是实测的 iroh-tickets 1.0.0 行为，**非文档承诺**。

## 陷阱

### 1. `TicketWireFormat::Variant1` 的 wire 判别符是 0x00，不是 0x01

`endpoint.rs:36-38`：

```rust
enum TicketWireFormat { Variant1(Variant1EndpointTicket) }
```

单变体 enum，postcard 按**位置**编号，故判别符 = 0。其自身测试向量（`endpoint.rs:203-207`）印证：`// variant` 对应 `"00"`。

对照 `iroh-blobs/src/ticket.rs:40-42` `enum TicketWireFormat { Variant0(Variant0BlobTicket) }`，测试向量（:227）写 `00 # discriminator for variant 0` —— **两者名字一个叫 Variant1 一个叫 Variant0，wire 上却都是 0x00**。

**照抄这个「单变体 enum」模式是对的**（它就是为留版本位而存在的，`iroh-blobs/src/ticket.rs:35-38` 注释：*"In the future we might have multiple variants (not versions, since they might be both equally valid), so this is a single variant enum to force postcard to add a discriminator"*），但务必**按位置而非名字理解判别符**：新增变体时它拿到的是 0x01。建议命名直接用 V0/V1 并配注释写明 wire 值。

### 2. wire format 跨版本断过 —— 连官方 README 都没跟上

`dumbpipe/README.md:47` 里印的真实 ticket（100 字符）用**当前 iroh-tickets 1.0.0 解不出来**：base32 解出 57 字节，但 postcard 反序列化失败（`Serde Deserialization Error`）。原因是首字节 = `0x20`（十进制 32，像是旧格式的 32 字节长度前缀），而 1.0.0 期望的判别符是 `0x00`。该 README 与 `dumbpipe/Cargo.toml:20` 声明的 `iroh-tickets = "1.0.0"` 不一致。

**两个教训**：
1. **ticket 字符串不是永久稳定的** —— n0 自己在 1.0 前就破坏过格式。若要「发出去的旧链接以后还能用」，必须自己扛版本兼容（单变体 enum 留位 + 老变体永不删）
2. **不要相信 iroh 文档里的示例 ticket 能跑** —— 以源码测试向量为准

### 3. BlobTicket 的编码是有损的

`iroh-blobs/src/ticket.rs:72-73` encode_bytes 内：
- `relay_url: self.addr.relay_urls().next().cloned()` —— **`.next()` 只取第一个**
- `direct_addresses: self.addr.ip_addrs().cloned().collect()`

wire 结构（:58-62）`struct Variant0AddrInfo { relay_url: Option<RelayUrl>, direct_addresses: BTreeSet<SocketAddr> }` —— **没有容纳 `TransportAddr::Custom` 的位置**（`relay_urls()`/`ip_addrs()` 是按变体过滤的 filter_map，`iroh/iroh-base/src/endpoint_addr.rs:137-152`，Custom 两个都不匹配、直接被滤掉）。

对照 `iroh-tickets/src/endpoint.rs:52-54`，EndpointTicket 存的是完整 `addrs: self.addr.addrs.clone()`（`BTreeSet<TransportAddr>`），**无损**。

> ⚠️ **多 relay 丢失在实践中影响有限**：iroh 自己的文档（`iroh-base/src/endpoint_addr.rs:146`）对 `relay_urls()` 说 *"In practice this is expected to be zero or one home relay for all known cases currently."* —— iroh 的寻址模型是 home-relay-singular。**Custom 地址丢失才是这条 finding 更真实的一半。**

**结论**：自定义 ticket 时直接存 `EndpointAddr`，别照抄 BlobTicket 的 Variant0 拆字段写法 —— 后者是历史包袱（`ticket.rs:44` 有显式 `// Legacy` 标记，结构体名 `Variant0NodeAddr` 与 `Variant0BlobTicket` 的 `node:` 字段（:47）都是遗留；注意 `endpoint_id` 反而是**当前**命名，不是旧名）。

### 4. 生成 ticket 前必须先 await online()

所有官方样例都带这个等待。dumbpipe 在全部 5 处生成 ticket 的路径上重复同一模式（`main.rs:364/472/550/648/770`）：

```rust
if (timeout(ONLINE_TIMEOUT, endpoint.online()).await).is_err() {
    eprintln!("Warning: Failed to connect to the home relay");
}
let addr = endpoint.addr();
```

注释（`main.rs:363`）：*"wait for the endpoint to figure out its home relay and addresses before making a ticket"*。

**两家超时行为不同**（别搞混）：
- dumbpipe：5s，**超时仅告警继续**
- sendme：30s（`main.rs:731-736`），**超时硬失败**（`.await?` 把 Elapsed 传进 anyhow）

> ⚠️ 「跳过 online() 就会产出连不上的废 invite」这个说法要加限定：id-only ticket **并非天生不可连** —— sendme `main.rs:660-662` 就是故意发 id-only ticket，靠 `PkarrPublisher::n0_dns()` 让它可用；`iroh/iroh/src/endpoint.rs:1036-1040` 也说没有地址的 EndpointAddr 仍可能靠 AddressLookup 连上。
>
> **准确表述**：跳过 online() 只在**没有配置 address_lookup / pkarr publisher** 时才产出废 invite。若这是你的默认配置，建议照 sendme 的做法：超时就拒绝生成 invite，别静默产出空 addrs。

## KIND 前缀是自描述类型标签

浏览器样例的用法（`iroh-examples/browser-chat/frontend/src/components/homescreen.tsx:15-20`）：

```ts
const [ticket, setTicket] = useState(() => {
  const url = new URL(document.location.toString())
  const ticket = url.searchParams.get("ticket")
  if (ticket?.startsWith("chat")) return ticket   // KIND 当廉价类型标签
  return ""
})
```

其中 `"chat"` 正是 `shared/src/lib.rs:55` 的 `const KIND: &'static str = "chat";`。

生成侧（`invitepopup.tsx:19-23`）走的是 **`?ticket=` 查询参数**，不是自定义 scheme：

```ts
function ticketUrl(ticket: string) {
  const baseUrl = new URL(document.location.toString())
  baseUrl.searchParams.set("ticket", ticket)
  return baseUrl.toString()
}
```

另见 `iroh-examples/dumbpipe-web/src/main.rs:113-123` `parse_subdomain`：先试 `iroh::EndpointId::from_str(subdomain)`，失败再试 `dumbpipe::EndpointTicket::from_str(subdomain)` —— 靠前缀+长度天然区分两种形态。

**自定义 ticket 只要 8~12 行**：`iroh-examples/browser-chat/shared/src/lib.rs:54-65` 的 ChatTicket impl 是 12 行（含 `#[derive]`），payload 只有 `topic_id` + `bootstrap: BTreeSet<EndpointId>`（**只有 id、没有地址** —— 那是因为 gossip 有自己的成员发现，点对点配对没有这个兜底，id-only 会强制你依赖 pkarr/DNS）。

> ⚠️ 别抄 browser-chat 的 `postcard::to_stdvec(&self).unwrap()` 错误处理（`shared/src/lib.rs:55` 区域）。

## iroh 生态没有「短码 → 地址」的 rendezvous

`n0-mainline/README.md` 原文：*"The main purpose for which iroh uses n0-mainline is endpoint address lookup via BEP_0044."* —— 即便 iroh 用到 Mainline DHT，用途也只是**按 endpoint 公钥查地址**，不是按任意短码查记录。

**iroh 的寻址原语只有 pubkey→addr（pkarr / DNS / mainline），没有 code→record 这层。**

**若要「人能念出来的短码」，只能自己维护一个 rendezvous 服务**（且要自己解决短码空间的可枚举问题——6 位数字码只有 10^6 空间，攻击者持续扫描即可捕获每一次进行中的配对；缓解手段是加长/加盐/限速/短 TTL）。**这是一笔明确的自维护基础设施成本，iroh 生态不提供任何对等物。**

**ticket 方案的对比**：「码」就是 32 字节 ed25519 公钥（EndpointId，`iroh/iroh-base/src/key.rs:70` `pub type EndpointId = PublicKey;`），**2^256 空间不可枚举，且根本不需要往任何公共存储写记录**（自包含）。

**换句话说：ticket 变长的那 57+ 个字符，买的正是「不可枚举 + 不用公开广播自己的地址」。** 若能接受放弃人念短码，改**二维码 + 链接 + 剪贴板**三件套即可（正是 sendme 做的 —— `main.rs:793-867` 有专门的剪贴板支持，feature 名就叫 `clipboard`）。

## 落地要点

| 决策点 | 建议 |
|---|---|
| **KIND 取值** | ⚠️ **必须先拍板再发版** —— KIND 会烤进每一个发出去的链接，改了就废掉所有存量 ticket。注意别与 URL scheme 冗余：若 scheme 是 `myapp://`，KIND 再取 `"myapp"` 会得到 `myapp://myapp<base32>` |
| **一次性 + 过期** | payload 里放 nonce + expires_at，`decode_bytes` 里用 `ParseError::verification_failed` 拒过期；**重放必须服务端记 nonce 已用集合**（`decode_bytes` 只能防「格式 / 时间」，防不住重放） |
| **长度预算** | 160~230 字符（id-only 63 + nonce/expires/元数据）。二维码 + 剪贴板 + 链接三件套，放弃人念 |
| **版本兼容** | 单变体 enum 留位，老变体永不删。**iroh 自己都破坏过一次**（见下） |
| **地址存法** | 直接存 `EndpointAddr`，**别拆字段**（BlobTicket 的拆字段写法是有损的，见下） |
| **online() 超时** | 照 sendme：超时就拒绝生成 ticket（除非确定配了 pkarr publisher） |
| **FFI 形状** | 抄 `iroh-ffi/src/ticket.rs:12-52`：`#[derive(Debug, uniffi::Object)]` + `#[uniffi::export(Display)]` + 两个 constructor（`from_addr` / `from_string`）。**ticket 是不可变值类型，用 Object 包裹比 Record 更省心**（避免每个字段都要过 FFI 类型映射）。⚠️ `iroh-ffi/Cargo.toml:5` 是 `publish = false`，它走 npm/maven/cocoapods 而非 crates.io —— **抄形状即可，不必依赖它** |

## 导航陷阱

`iroh-examples/browser-chat/browser-wasm/src/lib.rs:54` 的 `// let ticket = ChatTicket::new(topic);` 是注释掉的死代码；但 **:134 是 LIVE 代码**（`let mut ticket = ChatTicket::new(self.topic_id);`），真正的残留注释在 **:94**（`// ticket.bootstrap = [self.0.endpoint_id()]...`）。
