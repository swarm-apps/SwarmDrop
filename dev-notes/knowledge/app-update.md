# 应用内更新（SwarmHive）

三端的更新 UI 都由 **SwarmHive** 分发：SDK 是 npm 包 `@swarm-hive/sdk`（平台无关的 8 态
engine + ports），组件与平台 adapter 是两个 shadcn registry（`@swarmhive-rn` → `mobile/src`，
`@swarmhive` → 桌面 `src`）。**Web 端没有应用内更新**（刷新即最新）。

> 判断一个文件归谁：**头注释里有「由 registry 分发」声明的，一律不在本仓改**。就地改会在下次
> 拉取时被静默覆盖，且改动不会回流给其它 app。上游在 `../SwarmHive/packages/registry-{rn,web}/`。

**同步时的两个坑**：

1. **别对着上游逐字比对**。shadcn 拉取会剥掉文件头 banner，本仓 formatter 又会按自己的
   `printWidth` 重排（biome 换行时会补括号），所以「文本不同」是常态，不代表逻辑漂移。
   要比就先用同一份 formatter 归一化，否则写出来的校验只会一直误报。
2. **整文件覆盖会冲掉本仓的样式约定**。移动端的 `text-primary-ink`（State Ink Rule）与
   `text-[13px]` 字号在 registry 里是 `text-primary` / `text-xs` —— registry 是通用的，
   不知道下游的 token 约定。整文件同步后要回头把这两处改回来（2026-08-08 就栽过一次）。

## `ready` 是持久静止态，不是「正在等系统」

这是整套流程唯一容易读错的地方，读错就会做出死锁。

`ready` 陈述的是**本地事实**——「我手上有一个可安装的产物」。它与系统安装框弹没弹、用户点没点
**无关**，只在两种情况下失效：产物被装上（下次 check 判无更新），或产物损坏/过期
（`reconcile` 清掉）。由此有三条不变量：

| 不变量 | 含义 |
|---|---|
| **可恢复** | 产物在磁盘、元数据在 storage —— 进程重启后 check 一次即可回到 ready，不重下 |
| **幂等** | `install()` 是可反复调用的移交尝试，**不消耗** ready |
| **有出口** | 任何 UI 状态都必须至少有一个用户可操作的出口（No Dead End） |

把它读成「已移交，正在等结果」有两个前提，在 Android sideload 下都不成立：移交不保证成功
（见下条），结果也不保证回来（`ACTION_VIEW` 是 fire-and-forget，用户点取消收不到任何东西）。
一个等待外部结果、而外部既不保证接收也不保证回复的状态，就是死锁。

## Android 10+ 后台不能弹安装框，而且是**静默**失败

app 不在前台时 `startActivity` 会被 Background Activity Launch 限制丢弃：不抛异常、不回调，
只往 logcat 写一行 `Background activity launch blocked!`，而 `startActivityAsync` **照常
resolve**。v0.12.3 的现场就是这个：用户点更新后熄屏，下载完成 → 自动安装 → intent 被吞 →
UI 永久停在「系统弹窗确认中…」。

**正确做法**（都已落地）：

- `expo-installer` 派发前先看 `AppState.currentState === "active"`，不在前台就**不发**，抛
  `ApkInstallBlockedError("background")`。宁可不发，也不要发一个必然消失的 intent 换一句谎话。
- 自动安装的时机交给 `useAutoInstall`：**回前台那一刻**触发，每个 release 一次。
- 下载在后台完成时发一条通知（`notifier.ts` 的 `fireNotifyUpdateReady`）。这不只是提醒 ——
  **用户点击通知触发的 Activity 启动是 BAL 的合法例外**，回到前台后安装框就能正常弹。

**门禁失败 ≠ 安装失败**：intent 压根没发出去，产物完好，engine 应留在 ready。`useAutoInstall`
识别出 `ApkInstallBlockedError` 后调 `acknowledgeError()` 把状态推回去（engine 见句柄还在会
恢复到 `ready`）。

## 自动安装必须只有一个触发点

`ready → install` 的 effect 曾长在 prompt / force / settings 三个组件里，它们常常同时挂载。
从前没出事，是因为 engine「install 用掉即清句柄」**意外地**去了重；句柄改为可反复使用后
（幂等是上面那条不变量的要求），那层保护就没了，同一个 ready 会派发三次安装。

- **桌面**：编排上移到 `UpdateProvider`（天然单例），组件里一个 effect 都不留。
- **移动**：闸门与门禁结果提到 `use-auto-install.ts` 的**模块级**变量，用
  `useSyncExternalStore` 共享 —— 多个消费者只派发一次，且看到同一份提示。

## 状态判据要穷尽 8 态，不要「特判几个 + else 兜底」

两端设置页都栽过：`hasUpdate = available || force-required` 的二元判据让 `ready` 与
`downloading` 掉进兜底分支，于是产物已下好等着装的时候，页面显示「✅ 已是最新」，前面却压着一个
说有新版本要装的弹窗。

判据收在 `update-dialog-visibility.ts` 的 `updateActionKind(status)`：穷尽 switch + `never`
断言，将来新增状态在**编译期**报错，而不是运行时说谎。

## 续传做不到 —— expo 的 `resumeData` 只有 `pauseAsync()` 能产出

下载器名字叫 `createDownloadResumable`，看上去只差接线。**接完才发现那条路根本不通**：

```ts
// expo-file-system/src/legacy/FileSystem.ts
async pauseAsync() {
  const pauseResult = await ExponentFileSystem.downloadResumablePauseAsync(this.uuid);
  this.resumeData = pauseResult.resumeData;   // ← 全文件唯一一处赋值
  return this.savable();
}
savable() { return { url, fileUri, options, resumeData: this.resumeData }; }  // 只是读回来
```

我们的失败场景是**进程被杀**，没有任何钩子能在那之前调 `pauseAsync()`。所以"下载开始前存
`savable()`、下次拿它 `resumeAsync()`"存下来的 `resumeData` 恒为 `undefined` —— 原生层不带
`Range` 头，反而 truncate 目标文件。净效果是全量重下，外加一个残留文件和一份假装有用的存档。

**所以下载中断就是重下**，别在 UI 或文档里暗示别的。真要做只有一条路：`AppState` 切后台时
`pauseAsync()` 后立刻 `resumeAsync()` 刷出真实的 resumeData —— 那会打断后台下载，而"熄屏也要
能下完"正是这套流程的诉求。

**真正有效的那一半是 `reconcile`**：已完成并校验过的产物带一条记录
`{version, path, sizeBytes}` 落在 storage 里，下个进程 check 时复检（版本匹配 → 文件存在 →
尺寸 → ZIP magic）后直接进 `ready`，跳过整个下载阶段。`reconcile(release | null)` 一个方法
三种用途：匹配则恢复、不匹配则清理、传 null 则清空（Tauri 侧不实现，产物封在 plugin 内部）。

> 这个坑是被 code review 挖出来的，而当时**测试是绿的** —— mock 的 `savable()` 返回了一个
> 真实实现永远给不出的 `resumeData`。那组用例证明的只是 mock 自己的行为。
> 现在 mock 里保留 `resumeCalls` 作为反向护栏：它非空就说明有人又把这条路接回来了。

## 「安装未知应用」权限刻意不做门禁

`expo-intent-launcher` 不暴露 `canRequestPackageInstalls`，任何内建探测都是猜。猜错一次
（把"能装"判成"没权限"）就白白挡掉一次更新。未授权时照常派发 intent，Android 自己会把用户
领到授权页 —— 授权后返回，`ready` 还在，点「立即安装」即可。**这是通路，不是死路。**

推论：`ApkInstallBlockReason` 只有 `"background"` 一个取值。别为将来预先加一个代码产生不出来
的取值 —— 那会长出一条永远走不到的 UI 分支，并让人以为已经有对应的引导了。

## 相关文件

| 用途 | 路径 |
|---|---|
| 移动端更新 UI 宿主 | `mobile/src/components/update-host.tsx` |
| 移动端安装时机编排 | `mobile/src/hooks/use-auto-install.ts`（registry 分发） |
| 移动端就绪通知 | `mobile/src/hooks/use-update-ready-notification.ts` + `src/core/notifier.ts` |
| 移动端下载器 / 安装器 | `mobile/src/lib/expo-{downloader,installer}.ts`（registry 分发） |
| 桌面端更新 UI | `src/components/*update*.tsx`（registry 分发）+ `src/routes/_app/settings/-about-section.tsx` |
| 状态判据（两端同源） | `{mobile/,}src/lib/update-dialog-visibility.ts` |
| 变更提案 | `openspec/changes/update-flow-recovery/`、`../SwarmHive/openspec/changes/ready-state-durability/` |
