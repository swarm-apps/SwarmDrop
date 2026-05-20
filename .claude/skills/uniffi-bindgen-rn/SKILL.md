---
name: uniffi-bindgen-react-native
description: >
  Guide for bridging Rust code to React Native using uniffi-bindgen-react-native.
  Covers project setup, type mappings, async/Promise, callback interfaces,
  error handling, memory management, threading, multi-crate workspaces, and
  publishing. Also includes SwarmNote-specific bridging architecture.
triggers:
  - uniffi
  - uniffi-bindgen
  - rust bridge
  - react native rust
  - app-core
  - "#[uniffi::export]"
  - callback_interface
  - Turbo Module rust
---

# uniffi-bindgen-react-native 开发指南

当需要创建或修改 Rust ↔ React Native 桥接代码时，使用此 skill。

## 核心概念

uniffi-bindgen-react-native 基于 Mozilla UniFFI，通过 `#[uniffi::export]` 注解 Rust 代码，自动生成 TypeScript 类型 + C++ JSI 绑定，变成标准的 React Native Turbo Module。

调用链路：`TypeScript → Hermes JSI → C++ → Rust`，全程无 JSON 序列化。

## 参考资料

按需加载以下参考文件：

- **项目搭建与配置**：`references/setup.md` — 环境准备、项目创建、ubrn.config.yaml、CLI 命令
- **类型映射**：`references/type-mappings.md` — 标量、容器、Record、Object、Enum 的 Rust ↔ TS 映射
- **异步与回调**：`references/async-and-callbacks.md` — async/Promise、AbortSignal、callback_interface、Foreign Traits
- **错误与内存与线程**：`references/error-memory-threading.md` — 错误处理、GC、uniffiDestroy、线程死锁风险
- **多 Crate 与发布**：`references/multi-crate-and-publish.md` — 工作区结构、发布方式
- **SwarmNote 桥接设计**：`references/swarmnote-bridge.md` — app-core API 设计、Tauri → UniFFI 映射、RN 端使用示例

## 开发规范

1. 所有暴露给 RN 的 API 使用 `#[uniffi::export]`
2. 事件推送使用 `#[uniffi::export(callback_interface)]` trait，替代 Tauri 的 `app.emit()`
3. 状态管理使用 `Arc<Mutex<T>>` 独立持有，不依赖 Tauri AppHandle
4. 持有昂贵资源的 Object 应在 TS 端用 `uniffiDestroy()` 或 `uniffiUse()` 显式释放
5. 调用 JS callback 前必须释放 Mutex 锁，避免死锁
6. 错误类型使用 `instanceOf()` 静态方法判断，不使用 `instanceof`
7. 64 位整数在 TS 端为 `bigint` 类型
