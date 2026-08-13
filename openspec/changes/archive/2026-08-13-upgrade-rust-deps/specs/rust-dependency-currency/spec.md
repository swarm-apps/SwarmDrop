## ADDED Requirements

### Requirement: 安全/身份关键依赖维持受支持的最新版

项目 SHALL 把 sha2、chacha20poly1305、rmcp、keyring 这些安全/身份关键依赖维持在当前受支持的最新可用版本，并 SHALL 优先吸收上游的安全修复（如 rmcp v2 的 streamable-HTTP session leak 修复）。当某依赖处于上游唯一可用的预发布版（如 sea-orm 2.0 rc、specta rc）时，维持在该最新预发布版即视为满足本要求，不得为"求稳"而降级到更旧的稳定线。

#### Scenario: 升级后 Cargo.lock 解析到目标最新版
- **WHEN** 完成本次升级并执行 `cargo build --workspace`
- **THEN** `Cargo.lock` 中 sha2 ≥ 0.11.0、chacha20poly1305 ≥ 0.11.0、rmcp ≥ 2.0.0、keyring ≥ 4.1.2

#### Scenario: 未使用的死依赖被移除
- **WHEN** 审计发现某 crate 在目标 crate 代码中零引用（如 `src-tauri` 的 sha2）
- **THEN** 从对应 Cargo.toml 删除该死依赖，而非升级版本号

### Requirement: 加密与摘要行为跨依赖升级保持不变

RustCrypto 系依赖（sha2、chacha20poly1305）的升级 SHALL 保持所有可观测的密码学行为不变：SHA256 摘要值逐字节一致、XChaCha20Poly1305 加解密语义与确定性 nonce 派生不变，从而保证 DHT key 与已发布加密格式向后兼容、无需重置任何线路格式或重发 DHT 记录。

#### Scenario: SHA256 DHT key 与旧版逐字节一致
- **WHEN** 升级 sha2 0.11 后对同一 (namespace, id) 计算 DHT key
- **THEN** 结果与升级前完全一致，旧节点写入的 DHT 记录仍可被新版本检索

#### Scenario: 加解密往返成功且篡改被拒
- **WHEN** 升级 chacha20poly1305 0.11 后对一个 chunk 加密再解密
- **THEN** 还原出原始明文；且对密文做任意篡改后解密返回错误（AEAD 完整性校验生效）

#### Scenario: crypto 单元测试全绿
- **WHEN** 执行 `cargo test -p swarmdrop-core`
- **THEN** `transfer/crypto.rs` 内覆盖 roundtrip / 篡改 / 确定性 nonce 的全部单测通过

### Requirement: MCP server 工具与资源契约跨 rmcp 升级保持可用

rmcp 升级到 2.0 后，MCP server 对外暴露的工具调用与资源列举/读取契约 SHALL 保持可用且语义不变；本地 MCP 客户端在 v2 默认 Host/Origin 白名单下 SHALL 仍能连接，若默认白名单不覆盖实际绑定方式，则显式放行而非回退到无校验。

#### Scenario: 工具调用返回结果
- **WHEN** 本地 MCP 客户端在升级后调用任一已注册工具
- **THEN** 返回与升级前等价的结果内容（ContentBlock）

#### Scenario: 资源可被列举与读取
- **WHEN** 客户端列举并读取 `swarmdrop://` 资源
- **THEN** 返回与升级前一致的资源条目与正文

#### Scenario: 默认白名单下本地连通
- **WHEN** MCP server 以本地默认地址启动、客户端经 127.0.0.1/localhost 连接
- **THEN** 连接被接受；若被新白名单拒绝，则在 `StreamableHttpServerConfig` 显式放行后连接成功

### Requirement: 系统钥匙串身份读写跨 keyring 升级保持语义并经真机验证

keyring 升级到 4.x 后，release build 下的系统钥匙串身份读写语义 SHALL 保持不变（存入后可原样读出、不存在的条目返回 NoEntry 语义而非 panic）。由于该路径仅在 release build 生效且无法被 `cargo test` / dev build 覆盖，keyring 升级 SHALL 在合入前于 macOS、Windows、Linux 三平台的签名 release 包上完成身份读写与连续重启 PeerId 稳定性验证。

#### Scenario: 身份存入后可原样读出
- **WHEN** 在签名 release 包中存入设备身份，随后重新加载
- **THEN** 读回的密钥与存入的一致，PeerId 不变

#### Scenario: 缺失条目不导致崩溃
- **WHEN** 钥匙串中不存在身份条目（或新后端读不到旧条目）
- **THEN** 返回"未找到"语义、触发身份重建流程，不 panic、不阻塞启动

#### Scenario: 三平台真机验证为合入前置门
- **WHEN** keyring 后端升级（涉及 release-only 代码路径）准备合入
- **THEN** macOS / Windows / Linux 各自的签名 release 包均已通过身份读写 + 重启 PeerId 稳定性 smoke，未通过则不得合入
