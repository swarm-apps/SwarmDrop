## Context

SwarmDrop 当前没有任何版本更新机制。作为 P2P 文件传输工具，协议兼容性至关重要 — 当 libp2p 协议发生 breaking change 时，新旧版本客户端将无法通信。

当前技术栈：Tauri v2 + React 19 + Zustand 5。桌面端支持 Windows/macOS/Linux，移动端支持 Android（APK 直装，非应用商店分发）。

版本清单托管在 GitHub Releases。用户拥有阿里云 OSS + ECS 可作为备用 CDN，但本期不实现。

参考实现：[cc-switch](https://github.com/farion1231/cc-switch) 的 tauri-plugin-updater 集成方案。

## Goals / Non-Goals

**Goals:**
- 桌面端通过 tauri-plugin-updater 实现完整的自动更新（检测 → 下载 → 验证 → 安装 → 重启）
- 移动端通过 fetch latest.json 检测新版本，引导用户跳转浏览器下载 APK
- 支持强制更新（min_version），协议不兼容时阻断应用使用
- 启动时自动检查一次，设置页手动检查
- 设置页内嵌更新状态展示（无更新 / 有更新 / 下载中 + 进度条）

**Non-Goals:**
- 本期不实现 OSS/CDN 多源分发（后续支持）
- 不实现增量更新 / 差分更新
- 不实现 iOS 支持
- 不实现后台静默自动更新（需用户确认）
- 不实现应用内 APK 下载安装（Android 仅跳转浏览器）

## Decisions

### D1: 桌面端使用 tauri-plugin-updater

**选择**: `tauri-plugin-updater` v2 + `tauri-plugin-process`（重启）

**替代方案**:
- 自建 HTTP 下载 + 手动替换二进制：复杂度高，需处理文件锁、权限提升、签名验证
- Sparkle (macOS) + WinSparkle (Windows)：跨平台不统一，Tauri 已有官方方案

**理由**: Tauri 官方插件，与构建系统深度集成，自动生成 `.sig` 签名文件，支持 minisign 验证，跨平台统一 API。

### D2: 版本清单 latest.json 扩展移动端字段

**选择**: 在标准 `latest.json` 基础上，由 CI 组装时追加 `mobile` 字段：

```json
{
  "version": "1.2.0",
  "platforms": { ... },
  "mobile": {
    "android": {
      "version": "1.2.0",
      "download_url": "https://github.com/.../releases/download/v1.2.0/swarmdrop-1.2.0.apk",
      "min_version": "1.0.0"
    }
  }
}
```

**理由**: 复用同一 endpoint，桌面端 tauri-plugin-updater 会忽略未知字段，移动端自行解析 `mobile` 节点。

### D3: 强制更新通过 min_version 实现

**选择**: `latest.json` 中包含 `min_version` 字段（桌面端在 platforms 外层，移动端在 `mobile.android` 内）。应用启动时比较本地版本与 min_version，低于则弹出不可关闭的强制更新弹窗。

**替代方案**:
- 服务端 API 动态控制：需要中心化服务器，违背去中心化理念
- 本地硬编码最低版本：无法远程控制

**理由**: 利用已有的 latest.json 分发机制，无需额外基础设施。

### D4: 状态管理 — 新建 update-store

**选择**: 新建 `src/stores/update-store.ts`（Zustand），管理更新生命周期状态。

**状态定义**:
```typescript
type UpdateStatus =
  | 'idle'           // 未检查
  | 'checking'       // 检查中
  | 'up-to-date'     // 已是最新
  | 'available'      // 有更新可用
  | 'downloading'    // 下载中（桌面端）
  | 'ready'          // 下载完成待安装
  | 'error'          // 检查/下载失败
  | 'force-required' // 需要强制更新
```

**理由**: 更新逻辑独立于现有 store，状态机清晰，便于 UI 组件消费。

### D5: 更新检查时机

**选择**: 应用启动后延迟 3 秒自动检查一次 + 设置页手动触发。不做定时轮询。

**理由**: 启动检查覆盖日常使用场景；P2P 工具使用频率不高，轮询性价比低。延迟 3 秒避免阻塞启动流程。

### D6: 签名方案 — minisign

**选择**: 使用 Tauri 内置的 minisign 签名方案。公钥写入 `tauri.conf.json`，私钥存储在 GitHub Secrets（`TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）。

`tauri.conf.json` 中设置 `createUpdaterArtifacts: true`，构建时自动生成 `.sig` 文件。

### D7: UI 集成方式 — 设置页内嵌

**选择**: 更新 UI 嵌入设置页「关于」区域，非独立弹窗。强制更新使用全屏模态弹窗。

**参考**: cc-switch 的 `AboutSection.tsx` 方案 — 按钮文字状态变化 + 可展开的更新信息 banner。

UI 设计稿已在 `dev-notes/design/design.pen` 中完成。

## Risks / Trade-offs

- **[GitHub Releases 单点]** → 后续引入 OSS CDN 作为备用 endpoint，tauri-plugin-updater 原生支持多 endpoints 回退
- **[移动端无应用内安装]** → Android 跳转浏览器下载体验不如应用内安装，但避免了 `REQUEST_INSTALL_PACKAGES` 权限和安全审查
- **[签名密钥泄露]** → 私钥仅存在 GitHub Secrets，不进入代码仓库；公钥可公开
- **[强制更新误触发]** → min_version 由 CI 手动设置，非自动递增，降低误操作风险
- **[网络不可用时强制更新]** → 弹窗保持显示，用户恢复网络后可重试

## Open Questions

- CI/CD 工作流的具体结构（单 workflow 还是拆分 build/release）待实现时确定
- 阿里云 OSS CDN 的集成方案留待后续迭代
