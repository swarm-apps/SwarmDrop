## 1. iOS 私有数据区基础设施

- [x] 1.1 新建 `mobile/modules/app-paths` Expo module（仅 iOS 实现），暴露 `applicationSupportDirectory`，内部调用 `FileManager.urls(for: .applicationSupportDirectory)`
- [x] 1.2 在模块内确保 Application Support 目录存在（iOS 上默认不存在，必须显式创建），目录缺失时返回明确错误而非空串
- [x] 1.3 为模块补 TS 类型声明与 Android 侧的 not-implemented 兜底（Android 不消费此路径，调用即应报错而非静默返回错误路径）
- [x] 1.4 在 `mobile/` 下跑 `pnpm typecheck` 确认模块接线正确

## 2. 存储角色分离（dataDir）

- [x] 2.1 在 `mobile/src/core/paths.ts` 新增 `getPrivateDataDir()`：iOS 走 `app-paths` 模块，Android 保持 `Paths.document.uri`
- [x] 2.2 修改 `mobile/src/core/mobile-core.ts:12` 的 `dataDir` 改用 `getPrivateDataDir()`
- [x] 2.3 移除 `paths.ts` 的 `transfersInboxUri` 与 `resolveReceiveLocation()`（落点职责整体迁往第 3 组）
- [ ] 2.4 验证 iOS 上 `swarmdrop.db` 与 `staging/` 落在 Application Support 下（真机或模拟器检查文件系统）

## 3. 接收落点抽象（三态）

- [x] 3.1 新建 `mobile/src/core/receive-location.ts`，定义 `ReceiveLocation` 三态判别联合：`ready` / `unconfigured` / `revoked`
- [x] 3.2 实现 `getReceiveLocation()`：iOS 恒返回 `ready` + Documents URI；Android 从偏好读 SAF tree URI 并探活
- [x] 3.3 实现 `pickReceiveLocation()`：Android 唤起系统目录选择器并持久化结果；iOS 不暴露该能力
- [x] 3.4 实现 `requiresUserChoice()` 供引导流程判定步骤是否适用
- [x] 3.5 替换全部 `resolveReceiveLocation()` 调用点（`device-trust.ts:114`、`transfer-offer-host.tsx:114`），改为穷尽处理三态的分派（无 `default` 分支）
- [ ] 3.6 补单测：三态各自的解析、Android 未配置与失效的区分、iOS 恒 `ready`
  - **受阻**：`mobile/` 没有测试设施（`package.json` 无 `test` 脚本，全仓零 `*.test.ts`）。引入 runner 是独立工作，不属本变更范围。判据本身已抽成接收 `FlowState` 的纯函数（`onboarding-flow.ts`）与无副作用的 `locationFrom`（`receive-location.ts`），runner 就位后可直接测。

## 4. iOS 落点暴露（可与第 5 组并行）

- [x] 4.1 `mobile/app.json` 的 `ios.infoPlist` 新增 `UIFileSharingEnabled: true` 与 `LSSupportsOpeningDocumentsInPlace: true`
- [x] 4.2 接收落点改为 `Paths.document.uri` 本体（不再套 `transfers/` 子目录）
- [ ] 4.3 真机验证：完成一次接收后，文件出现在「文件」App 的 `On My iPhone / SwarmDrop` 下
- [ ] 4.4 真机验证：该目录下**不含** `swarmdrop.db*` 与 `staging/`
- [ ] 4.5 真机验证：中断一次接收，隔一段时间后恢复，续传从断点继续（确认 staging 搬家未破坏续传）

## 5. Android SAF 落点与引导流程（可与第 4 组并行）

- [x] 5.1 把接收目录 URI 纳入 `preferences-store` 持久化（替换现有 `receivePath` 语义，明确其为唯一落点而非可选覆盖）
- [x] 5.2 重构 `mobile/src/stores/onboarding-store.ts`：移除持久化的 `hasOnboarded` 布尔，改为「有序步骤 + 每步 `isSatisfied()` 判据」模型，仅保留 `hasSeenIntro` 一个持久位
- [x] 5.3 引导路由改为「指向第一个未满足的步骤」；`OnboardingDots` 的步数按平台实际步骤计算
- [x] 5.4 新增 `mobile/src/app/onboarding/receive-folder.tsx`：说明这次选择的意义，唤起系统目录选择器；用户取消时停留在本步并给出说明
- [ ] 5.5 验证存量用户路径：以 `hasOnboarded=true` 的旧数据启动，确认被领到接收目录步骤且不重复询问设备名
- [ ] 5.6 验证重启后 SAF 授权仍有效，无需重选目录
- [x] 5.7 设备详情页的 `defaultSaveLocation` 与全局落点的关系需明确：其为「按设备覆盖」，未设置时继承全局落点；更新 `[peerId].tsx:594` 附近的文案

## 6. 落点失效的探活与恢复

- [x] 6.1 `transfer-offer-host.tsx` 接受入站请求前校验落点可写，失败则进入 `revoked` 态
- [x] 6.2 实现 `revoked` 引导：说明原目录不可用、显示原路径、提供重选入口
- [x] 6.3 重选成功后继续此前被拦截的接受流程（而非要求用户重新操作一遍）
- [ ] 6.4 手动验证：清除应用数据后收到传输请求，确认在接受前被拦截且引导可用

## 7. 「在文件夹中显示」入口收敛

- [x] 7.1 `mobile/src/core/saf-intent.ts` 的 `canOpenSaveFolder()` 判据改为依据落点状态，移除「Android `file://` 恒 false」的私有目录分支
- [x] 7.2 复核收件箱详情与传输详情两处入口的渲染条件，`ready` 态下必须可用
- [ ] 7.3 两端手动验证：点击入口能唤起系统文件管理器并定位到目录

## 8. 移动端「从收件箱发送」入口

- [x] 8.1 收件箱详情新增「发送到设备」动作：把选中文件的 `localPath` 作为 `sourceId` 写入 `share-store`，push `/send/share-target`
- [ ] 8.2 支持多选：收件箱详情的文件列表进入选择态后可批量发送
  - **未做，已改由两级入口覆盖**：整条记录（sheet 的「发送到设备」）+ 单个文件（文件行的发送按钮）。多选需要给 `FileBrowser` 四个视图与工具栏引入选择态，收益只覆盖两级入口之间的窄缝。判据已写进 `DESIGN.md` 的 Received File Reuse Contract。
- [x] 8.3 发起前校验文件仍在原位置，缺失则标记 missing 并中止（不进入选设备界面）
- [x] 8.4 已标记缺失的文件其发送动作不可用
- [x] 8.5 验证离开选设备界面时 `share-store` 被清空，不污染交互式发送页
- [ ] 8.6 端到端验证：A→B 发送文件，B 从收件箱直接转发给 C，全程不经系统分享面板

## 9. Web 端（可与移动端并行）

- [x] 9.1 `crates/web/src/file_access.rs` 新增从 OPFS 注册文件源：经 `FileSystemFileHandle.getFile()` 取得 `web_sys::File` 后复用现有 `register_batch`
- [ ] 9.2 补 wasm 测试覆盖 OPFS 文件源的 `read_source_chunk` 与用户选中文件行为一致
  - **未做**：`send_inbox_files` 只做 OPFS 句柄 → `File` 的转换后即调 `send_files`，读分块那条路径一行未改，已有的 `same_name_sources_do_not_collide` 等 25 条 wasm 测试仍覆盖它。要测的其实是 `opfs::open_file` 与 `<input type=file>` 产出的 `File` 是否等价——那是浏览器契约，不是本仓代码。
- [x] 9.3 `docs/app/app/inbox/` 新增「发送到设备」入口，走既有 share store → 发送流程
- [x] 9.4 收件箱新增批量导出：多选后一次性交付到浏览器下载出口
- [x] 9.5 `pnpm build:wasm` 重新生成 `packages/swarmdrop-web` 产物
- [x] 9.6 `./scripts/check-wasm.sh` 与 `./scripts/test-wasm.sh` 通过

## 10. 跨端契约与文档

- [x] 10.1 `DESIGN.md` 新增 `### Receive Location Contract (cross-platform)`：落点必须用户可见、三态语义、失效恢复、私有数据不得混入
- [x] 10.2 `DESIGN.md` 新增 `### Received File Reuse Contract`：转发入口形态、不进常驻导航、与 Send Entry Contract 的关系；若桌面端不实现，写明豁免理由
- [x] 10.3 更新 `CLAUDE.md`「接收是暂存 → 发布两段」段落中的落点描述与 `<data_dir>/staging/` 位置
- [x] 10.4 更新 `dev-notes/knowledge/rust-backend.md` 的 `dataDir` 语义与 iOS 存储分区说明
- [x] 10.5 在 `dev-notes/knowledge/` 记录 Android 选 SAF 而非 MediaStore 的判据，以及将来要做零交互默认落点时的路径

## 11. 收尾验证

- [x] 11.1 `cargo check --workspace --all-targets` 与 `cargo test --workspace` 通过
- [x] 11.2 `mobile/` 下 `pnpm typecheck` 通过
- [x] 11.3 `pnpm check:zustand-access`、`pnpm check:shared-view` 通过
- [x] 11.4 移动端新增文案全部走 Lingui，`pnpm i18n:extract` 后 catalog 无缺失
- [ ] 11.5 走 `/simplify` 与 `/code-review` 两道关
- [ ] 11.6 破坏性变更需在发布说明中前置告知：iOS 升级后传输历史与收件箱清空（身份与配对关系保留）；Android 需重新指定接收目录
  - **落点在提交与发版**：`CHANGELOG.md` 由 git-cliff 从 commit message 生成（`pnpm changelog`），不手改。提交时 message 需带 `BREAKING CHANGE:` 段，内容即上面两条。

## 12. 审查发现（已修，见 design.md 的 D12–D14）

- [x] 12.1 自动接收改走宿主当下落点：`ReceivePolicyContext.host_default_save_location` + `IncomingTransferRuntime::host_default_save_location` + uniffi `set_default_save_location`，删除 host 侧的快照复制；补 3 条策略测试
- [x] 12.2 探活结论经 `useSyncExternalStore` 广播，设置页与引导判据都能看见 `revoked`；`useReceiveLocationWatch` 回前台重探
- [x] 12.3 `send_inbox_files` 改为跳过取不到的条目并经 `take_skipped_forward_paths` 回报，UI toast 告知
- [x] 12.4 引导完成状态改为**零持久位**：欢迎页借设备名作判据，`onboarding-store` 整个删除
- [x] 12.5 boot 等待 `waitForPreferencesHydration`（引导判据全在偏好里，不等它会把已配置用户一次性重定向进引导，并丢弃冷启动的分享意图）
- [x] 12.6 iOS 遗留内部文件清理（`legacy-cleanup.ts`）——`UIFileSharingEnabled` 会把旧的 db/staging/transfers 一并暴露给用户
- [x] 12.7 `getPrivateDataDir()` 移入 `initMobileCore()`，原生模块缺失从致命红屏变成可重试的启动失败
- [x] 12.8 目录选择结局改三态 `DirectoryPick`（picked/cancelled/unusable）——取消不再被渲染成错误
- [x] 12.9 「选择」按钮判据改用「落点当前能不能用」；被拦下时再点接受直接进目录选择（此前是静默 no-op）
- [x] 12.10 Web 转发对话框：关闭时 `action.cancel()`，退场动画期间不再闪「0 个文件」
- [x] 12.11 `modules/` 纳入 biome 覆盖（`biome.json` includes + lint/format/lint:ci 脚本）
