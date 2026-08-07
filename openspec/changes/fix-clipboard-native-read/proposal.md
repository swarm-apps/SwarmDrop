## Why

桌面壳读剪贴板走的是 WebView API，写走的是原生 plugin —— 一半守住了，一半漏了。

- `src/hooks/use-clipboard-invite.ts:26` 用 `navigator.clipboard.readText()`。
- 而 `src/lib/clipboard.ts` 的文档注释已经写明了为什么不该用 WebView API：
  「避免 `navigator.clipboard.writeText` 触发浏览器权限申请弹窗（桌面 app 里体验很怪异）」。
  同一个理由对读路径成立，甚至更强 —— 读比写的权限门槛高。
- `src-tauri/capabilities/default.json:45` 只有 `clipboard-manager:allow-write-text`，
  **没有 read-text**。这条缺失本身就是证据：读从来没走过 plugin，一直靠 WebView 兜。
- 兜的结果是失败被吞掉：`use-clipboard-invite.ts:27` 的 `catch { return }` 把
  「WebView 拒绝读剪贴板」和「剪贴板本来就是空的」压成同一个静默分支。
  剪贴板感知在某些平台从来没生效过也不会有任何信号。

这是一条独立的 bug，不依赖任何设计决策，先合掉不阻塞后续 change。

## What Changes

- `src-tauri/capabilities/default.json` 加 `clipboard-manager:allow-read-text`。
- `src/lib/clipboard.ts` 加 `readText()`，与既有 `copyText()` 对称 —— 剪贴板读写在这一个
  文件收口，调用方不直接碰 plugin 也不碰 `navigator.clipboard`。
- `use-clipboard-invite.ts` 改走它；把「读失败」与「剪贴板为空」分开，读失败记一条
  `console.debug`（不弹用户可见错误 —— 剪贴板感知是增强路径，失败应静默降级，但不该无痕）。
- 加机器兜底：`scripts/check-clipboard-access.mjs` 扫 `src/` 里对 `navigator.clipboard` 的
  直接引用并 fail，接进 `package.json`。与 `check:zustand-access` 同一个套路 ——
  这类「必须走某个封装」的约束靠人审守不住，仓里已有先例。

**非目标**：移动端剪贴板（`expo-clipboard` 本来就是原生 API，没有这个问题）；
Web 端（浏览器里 `navigator.clipboard` 是唯一选择，正确）；剪贴板检测的范围与呈现形态
（属 `pair-deep-link` change）。

## Capabilities

### New Capabilities

- `clipboard-native-access`: 桌面壳的剪贴板读写一律经原生 plugin，收口在单一封装模块，
  WebView 剪贴板 API 在桌面代码里被机器检查禁止。

## Impact

- `src-tauri/capabilities/default.json`（+1 权限）
- `src/lib/clipboard.ts`（+1 导出）
- `src/hooks/use-clipboard-invite.ts`（换调用 + 错误分支）
- `scripts/check-clipboard-access.mjs`（新增）+ `package.json`（+1 script）
- 回归：桌面「复制邀请 → 切窗回来 → 一键条出现」冒烟；`pnpm exec tsc --noEmit`

**风险**：`allow-read-text` 扩大了 webview 的能力面。桌面壳的 webview 只加载本地打包资源
（无远程内容、无第三方 iframe），拿到剪贴板读权限的只有自己的前端代码，风险可忽略。
