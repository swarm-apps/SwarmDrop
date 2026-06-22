# 桌面端免密码启动

OpenSpec 变更 `extract-core-and-add-rn-mobile` 的任务 6.x 取消了桌面端"每次启动都要输入应用密码"的强制流程。本文档描述新行为以及可选的本地锁定入口。

## 新启动流程

1. 应用启动时初始化 `KeychainProvider`（桌面侧使用 `keyring` crate）。
2. core runtime 先尝试从 keychain 读取 Ed25519 设备 identity；不存在时立即生成并保存，**不再要求用户设置应用密码**。
3. 已配对设备从 backend 数据库读取，不再依赖前端 Stronghold hydration。
4. 网络节点自动启动，前端直接进入 `/devices` 页。

## 旧 Stronghold 数据的处理

- 任务 6.3 / 6.4 直接清理了旧的 Stronghold 启动路径，**没有迁移分支**。
- 如果用户的旧 vault 仍存在，core 会忽略它并在 keychain 中重新生成 keypair，PeerId 因此会变。这是有意为之的破坏性变更，已在 release notes 中说明。
- 任务 6.5 保证：首次启动或检测到旧数据时，直接生成新的 identity 并写入 keychain，不阻塞 UI。

## 路由 / 守卫调整

- 任务 6.6 删除了 `_auth/setup-password.lazy.tsx`、`_auth/unlock.lazy.tsx` 的强制守卫。
- 仅保留 `enable-biometric.lazy.tsx` 作为**可选**的本地锁定入口（任务 6.7）：用户可以在设置中启用生物识别锁定应用 UI，但这只是 UI 屏蔽层，**不再绑定设备身份的解密**。
- 启动时 `useAuthStore.beforeLoad` 守卫只检查"是否启用了可选锁定"，未启用时直接放行到 `/devices`。

## 验证点

- 任务 6.8：连续重启应用，PeerId 必须保持稳定（验证 keychain 持久化）。
- 任务 11.1：首次启动 UI 不再要求输入密码。
- 任务 11.2：迁移场景下，旧 Stronghold 数据被忽略，新 PeerId 由 keychain 生成。

## 相关代码

| 关注点 | 位置 |
| --- | --- |
| 桌面 keychain 实现 | `src-tauri/src/host/keychain.rs` |
| 已配对设备数据库存储 | `crates/core` + `crates/entity` |
| 可选生物识别入口 | `src/routes/_auth/enable-biometric.lazy.tsx`、`src/stores/auth-store.ts` |
| Core 身份初始化 API | `swarmdrop_core::host::KeychainProvider`、core runtime `initialize_identity` |

## 相关文档

- [Core / Desktop / Mobile 架构边界](./core-desktop-mobile-boundaries.md)
- [Core 抽离盘点](./core-extraction-inventory.md)
