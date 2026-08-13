# Tasks — update-flow-recovery

> **依赖**：任务 2 起阻塞于 SwarmHive 仓的 `ready-state-durability` change 完成并发布。
> 任务 1、3、4 与上游无关，可先行。

## 1. 弹层 responder 修复（本仓独有，无上游依赖）

- [x] 1.1 验证前提：`@rn-primitives/dialog` 的 `Content` 恒真
      `onStartShouldSetResponder` 是否真的必要 —— 写一个最小复现，确认去掉后点击弹窗内容
      不会冒泡到 `Overlay.onPress` 误关弹窗
- [x] 1.2 `mobile/src/components/ui/dialog.tsx` — 在 `DialogPrimitive.Content` 上覆写
      `onStartShouldSetResponder`（`{...props}` 在其后，故本仓传值即可覆盖，无需 patch
      node_modules）
- [x] 1.3 ~~`alert-dialog.tsx` 同 1.2~~ —— **不需要**：`@rn-primitives/alert-dialog` 的
      `Content` 是纯 `View`，没有 responder 抢占（只有 `dialog` / `popover` / `select` /
      `tooltip` 有）。所以 force 弹窗与进度弹窗里的 ScrollView 本就能滚，卡住的只有走
      `Dialog` 的 prompt 弹窗 —— 正是用户截图那一个
- [x] 1.4 回归验证：更新弹窗的 release notes 可滚；设备详情弹层、其它含 `ScrollView` 的
      弹窗未被本改动影响；点击弹窗内容不误关
- [x] 1.5 在 `mobile/src/components/ui/dialog.tsx` 留一条注释说明覆写原因与上游行为，
      避免下次升级 `@rn-primitives/*` 时被当成冗余代码删掉

## 2. 消费上游 registry

- [x] 2.1 `mobile/package.json`（`^0.4.0`）与根 `package.json`（`^0.1.0`）的
      `@swarm-hive/sdk` 都已提到 `^0.5.0`
- [x] 2.1b SwarmHive 已打 `sdk/v0.5.0` tag，`publish-sdk.yml` 发布成功，npm 上
      `@swarm-hive/sdk` 已是 `0.5.0`；本仓两处 `pnpm install` 已更新 lockfile
      （根与 mobile 都指向 `0.5.0`），并在真实依赖下跑过全量门禁
- [x] 2.2 重新拉取 `@swarmhive-rn` registry 到 `mobile/src/`：`lib/expo-downloader.ts`、
      `lib/expo-installer.ts`、`lib/rn-adapter.ts`、`lib/ports.ts`、
      `lib/update-dialog-visibility.ts`、`lib/update-texts.ts`、
      `components/{prompt,force}-update-dialog.tsx`、`components/update-progress-dialog.tsx`、
      `components/update-settings-section.tsx`，以及新增的 `hooks/use-auto-install.ts`
- [x] 2.3 重新拉取 registry-web 到 `src/`：`lib/tauri-adapter.ts`、
      `lib/update-dialog-visibility.ts`、`components/*update*.tsx`
- [x] 2.4 `mobile/src/components/update-host.tsx` — 组件内的 `useEffect(ready → install)`
      已随 registry 拉取一并移除（编排上移到 `useAutoInstall`，门禁错误也由它内部识别并
      把 engine 推回 ready）；宿主这边补的是进度弹窗的 `onDismiss` 出口
- [x] 2.5 `mobile/src/lib/update-texts.ts` 是 registry 分发文件 —— 确认拉取后本仓的
      `zh-Hans` 文案仍然是项目要的口径（`resolveUpdateTexts` 的 overrides 机制可就地覆盖，
      **不要直接改文件**）
- [x] 2.6 确认拉取后没有遗留本仓就地修改（`git diff` 逐一核对被覆盖文件）

## 3. 两端设置页判据穷尽 8 态

- [x] 3.1 `mobile/src/app/settings/about.tsx` — 把
      `hasUpdate = available || force-required` 的二元判据换成对 `UpdateStatus` 的穷尽
      switch，`ready` 独立分支渲染可点的安装入口
- [x] 3.2 `src/routes/_app/settings/-about-section.tsx` — `UpdateButton` 的
      `case "downloading": case "ready":` 拆开，`ready` 分支为启用态的「立即安装」
- [x] 3.3 `src/routes/_app/settings/-about-section.tsx` — `DownloadProgressBanner` 在
      `ready` 态改口（停转圈、去速度读数、文案「更新已就绪」）
- [x] 3.4 两端都加 exhaustive check（`satisfies never` 或等价），让将来新增状态在编译期报错
- [x] 3.5 i18n：新增文案走 Lingui，桌面 `pnpm i18n:extract`、移动 `mobile/` 下同步

## 4. Android 下载完成通知

- [x] 4.1 通知本体落在 `mobile/src/core/notifier.ts`（而非新建 `lib/update-notification.ts`）：
      渠道常量与「只检查不请求」的权限探测都是它的私有实现，为复用而导出内部件不如让它多认
      一种同类告警 —— 配对 / 传输 / 更新就绪都是「app 不在前台时发生了一件等你点头的事」。
      React 侧编排单独成 `mobile/src/hooks/use-update-ready-notification.ts`
- [x] 4.2 通知点击 → 拉起应用主 Activity（回前台后由 `useAutoInstall` 接手，本仓不写安装逻辑）
- [x] 4.3 权限走**只检查不请求**的变体；未授权时静默跳过，不弹权限框
- [x] 4.4 `mobile/src/components/update-host.tsx` — 接线 4.1，并在离开 `ready` 时撤销通知
- [x] 4.5 iOS 全 no-op

## 5. 验收（故障复现路径必须走通）—— **需真机 Android，尚未执行**

- [ ] 5.1 有新版本 → 点更新 → **熄屏** → 等下载完 → 亮屏回到 app → 系统安装框自动弹出
- [ ] 5.2 熄屏期间收到「新版本已下载」通知；点通知回到 app 后安装框弹出
- [ ] 5.3 在系统安装框点「取消」→ 回到 app → 「已取消，可重试」+ **可点的**「立即安装」
- [ ] 5.4 下载完成后杀进程 → 重开 → check 后**直接 ready**，不重新下载
- [ ] 5.5 下载到一半杀进程 → 重开 → 残留被清掉、全量重下（**不做续传**，见上游 design D7）
- [ ] 5.6 更新弹窗的 release notes **能滚动**
- [ ] 5.7 `ready` 态下关于页显示安装入口，**不显示「已是最新」**
- [ ] 5.8 未授予「安装未知应用」权限时，Android 自己的授权页正常出现；授权返回后 ready 仍在、点「立即安装」可装
- [ ] 5.9 桌面端：ready 态主按钮可点；进度弹窗 Esc / 点外部可关且下载不中断
- [ ] 5.10 iOS：关于页无「软件更新」分组，无任何更新相关通知

## 6. 收口（三道关）

- [x] 6.1 机器门禁：`pnpm check:zustand-access`、`pnpm test`、`pnpm build`；
      `mobile/` 下 `pnpm typecheck`
- [x] 6.2 `/simplify` —— 4 个并行 agent（reuse / simplification / efficiency / altitude）。
      findings 收敛到同一个结构问题：`useAutoInstall` 既驱动安装又被 4 个消费者各挂一份，
      由此长出模块级手写 store、零调用者的测试钩子、3 份必然 no-op 的 AppState 监听。
      根治：编排上移到 `UpdateProvider` 单点，hook 瘦成只读；顺带 `progressView` 统一四处
      不一致的进度派生、`onDismiss` 改必填、notifier 三份重复 preamble 提取
- [x] 6.3 `/code-review high` —— 14 条，两条会让功能不成立：假续传（已整段撤销）与
      SDK 版本未升（桌面装的是 0.1.0，install 仍清句柄，所有「立即安装」按钮是 no-op）。
      其余真 bug 5 条、清理 4 条、已在 review 期间修掉 3 条、误判 1 条
- [x] 6.4 更新知识库：
      · `mobile/dev-notes/knowledge/theme-and-styling.md` 增补「`@rn-primitives/dialog` 的
        Content 会掐死弹窗内所有 ScrollView」（含「只有 dialog 有、alert-dialog 没有」这条判据）
      · **新建** `dev-notes/knowledge/app-update.md` —— 三端更新流程的架构约束（ready 静止态 /
        BAL 静默失败 / 自动安装单点 / 判据穷尽 / 续传），并挂进 `CLAUDE.md` 的知识库索引
- [ ] 6.5 归档 `mobile-version-check` spec 的移除（`openspec` 归档流程）
