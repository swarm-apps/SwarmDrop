## 1. API 与配置模型

- [ ] 1.1 扩展 `NodeConfig`，新增 data-channel 协议注册、open timeout、idle timeout、per-peer limit、per-protocol limit 配置
- [ ] 1.2 定义 `DataChannelId`、`DataChannelDirection`、`DataChannelProtocolConfig`、`DataChannelLimits` 等通用类型
- [ ] 1.3 定义 typed `NetworkFailureKind`、`DataChannelCloseReason`、`DataChannelError`，替换或补充 string-only failure
- [ ] 1.4 为 request-response 定义 `RequestOptions`，支持 per-call timeout 和 correlation metadata

## 2. DataChannelBehaviour 核心

- [ ] 2.1 新建 data-channel runtime 模块，定义 `DataChannelBehaviour` 和 `DataChannelHandler` 的边界
- [ ] 2.2 在 `CoreBehaviour` 中组合 data-channel behaviour，并保持 ping、identify、kad、req_resp、mdns、relay、autonat、dcutr 现有行为
- [ ] 2.3 实现出站通道打开流程：协议协商、channel ID 分配、open timeout、取消处理
- [ ] 2.4 实现入站通道接收流程：协议匹配、limit 校验、channel handle 生成、拒绝原因归一化
- [ ] 2.5 实现通道关闭和错误归一化，将底层连接关闭、协议协商失败、资源限制、本地取消映射为 typed reason

## 3. Command 与 Client 集成

- [ ] 3.1 新增 `OpenDataChannelCommand`，通过现有 command loop 打开出站数据通道
- [ ] 3.2 扩展 `NetClient`，提供 `open_data_channel(peer_id, protocol, options)` API
- [ ] 3.3 扩展 request-response command，使 `send_request_with_options` 能应用 per-call timeout
- [ ] 3.4 确保 command future 被 drop 或取消时，event loop 能清理对应 active command

## 4. Runtime Event 与消费路径

- [ ] 4.1 定义非序列化的 data-channel runtime event 或 in-process receiver，避免把 stream handle 塞进 UI event
- [ ] 4.2 为入站通道、出站打开失败、通道正常关闭、通道错误发出 typed lifecycle event
- [ ] 4.3 更新 `NodeEvent` 或新增并行事件类型，使普通网络事件和 data-channel handle 事件边界清晰
- [ ] 4.4 更新 `EventReceiver` 或新增 receiver，使调用方能同时消费普通节点事件和数据通道事件

## 5. 测试与验证

- [ ] 5.1 添加单元测试覆盖 data-channel limit 校验、typed error 映射和配置默认值
- [ ] 5.2 添加双节点集成测试：出站打开通道、入站接受通道、双向字节读写、正常关闭
- [ ] 5.3 添加失败场景测试：unsupported protocol、open timeout、peer disconnect、local cancel、resource limit exceeded
- [ ] 5.4 添加 request-response options 测试，确认默认 timeout 和 per-call timeout 都生效
- [ ] 5.5 更新 `swarm-p2p-core` 示例或测试辅助代码，展示如何注册协议并打开通用数据通道

## 6. 下游适配准备

- [ ] 6.1 更新 `crates/core` 中对 `NetClient` typed error 的编译适配，但不引入文件传输数据通道逻辑
- [ ] 6.2 更新文档注释，明确 `libs/core` 只提供 transport primitive，应用协议在下游实现
- [ ] 6.3 运行 `cargo test` 覆盖 `libs/core` 与受影响的 core crates
