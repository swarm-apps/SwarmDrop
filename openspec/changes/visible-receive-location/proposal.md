## Why

移动端的接收落点默认是**应用私有目录**（`mobile/src/core/paths.ts:28` 的 `resolveReceiveLocation()` 回退到 `<documentDirectory>/transfers`），而发送页的四个来源全部是系统 picker（`DocumentPicker` / `pickDirectoryAsync` / 相册）。这两个集合**不相交**且是平台硬约束：Android 的 SAF 不暴露任何 app 的私有目录；iOS 的 `Documents` 在未声明 `UIFileSharingEnabled` 时不出现在「文件」App 里。

后果是收到的文件成了孤岛——既进不了系统文件管理器（`mobile/src/core/saf-intent.ts:52` 的 `canOpenSaveFolder()` 对 Android 私有目录直接返回 `false`，「打开文件夹」入口根本不渲染），也没有任何正经路径能把它转发给第三台设备。唯一走得通的是「收件箱 → 系统分享面板 → 在一堆 app 里找到 SwarmDrop 自己 → share-target」，要求用户把文件分享给正在用的这个 app，且 Android 私有目录下会产生两次额外拷贝。

根因是桌面端的一条隐含前提被移动端打破了：桌面的接收目录天然用户可见，所以「系统 picker 选得到收到的文件」不言自明。移动端照搬了 UI 契约却没有这个前提。Web 端的 OPFS 同样不可见，但性质不同——它是浏览器配额存储，规范上就定义为对用户不可见，其正确出口是下载而非磁盘路径。

## What Changes

- **拆分 `dataDir` 与 `saveDir` 两个角色。** 二者当前纠缠在同一个 `Paths.document.uri` 下：`swarmdrop.db`（`mobile/src/core/mobile-core.ts:12`）、`staging/`（`file_staging.rs:35`）与 `transfers/`（`paths.ts:14`）同住一处。拆分后 `dataDir` 恒为应用私有数据区，`saveDir` 恒为用户可见接收区。

- **iOS：`Documents` 成为用户可见接收区。** 声明 `UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace`；`dataDir` 改指 `Library/Application Support/`，db 与 staging 随之搬离；接收落点直接是 `Documents` 根，不再套 `transfers/` 子目录（「文件」App 已提供 `SwarmDrop` 这一层）。

- **Android：接收目录改为用户选定的 SAF tree，且不再有私有目录回退。** `resolveReceiveLocation()` 不得再静默落到私有目录。目录选择进入**首启引导**，成为与设备命名并列的一步。

- **消除 `save_dir` 的第三态。** 治本后 iOS 恒 `file://`、Android 恒 `content://`，`mobile/packages/swarmdrop-core/rust/mobile-core/src/file_access.rs:224` 的分支判据从「碰运气分流」变为「平台判据」，两条 publish 路径各自对应一个平台。

- **Web：确立「OPFS 暂存 → 下载发布」形态。** 不引入 File System Access API（Safari 与 Firefox 全版本不支持目录 picker，引入即意味着两套 sink 实现或部分浏览器失去接收能力）。OPFS 是持有区，下载是发布出口，与 `receive-staging-publish` 的两阶段模型同构。

- **三端补齐「从收件箱直接发送」入口。** 复用已有的反向流（文件已定 → 挑设备）：收件箱选中 → `share-store` → share-target。移动端后端零改动（`readSourceChunk` 已对 `file://` 与 `content://` 一视同仁）；Web 端只需在注册文件源前多一次 `FileSystemFileHandle.getFile()`。

- **BREAKING：不做数据迁移。** iOS 上 db 换家等于传输历史与收件箱清空（身份存于 keychain，不受影响）；Android 上存量文件仍在旧私有目录、新落点在别处，存量收件箱记录将指向不可见文件。当前用户基数极小，迁移代码的长期成本高于其收益。

- **BREAKING：Android 未选择接收目录时不能接收。** 这是消除私有目录回退的直接后果，需在接受入站请求前拦截并引导。

## Capabilities

### New Capabilities

- `visible-receive-location`: 接收落点必须落在用户可见位置的跨端契约——`dataDir`/`saveDir` 的角色分离、各平台的落点判据、引导流程中的目录选择、SAF 授权的生命周期与失效恢复、Web 端的暂存/发布形态。
- `received-file-reuse`: 已接收文件的二次流转——三端从收件箱直接发送到另一台设备的入口形态、文件源标识的复用规则、以及「在文件夹中显示」入口的可用性判据。

### Modified Capabilities

（无。现有 spec 中与本变更相关的 `file-sink` 描述的是 Rust 侧 sink 的写入与最终化机制，本变更不改动其需求；`inbox-item-presentation` 唯一的需求是条目级内容类型判定，与新增动作无关。）

## Impact

**移动端配置与装配**
- `mobile/app.json` — `ios.infoPlist` 新增两个键
- `mobile/src/core/mobile-core.ts` — `dataDir` 平台分支
- `mobile/src/core/paths.ts` — `resolveReceiveLocation()` 语义变更，私有目录回退移除
- 新增 tiny Expo module 暴露 iOS Application Support 路径（`mobile/modules/` 下，与既有 `content-share`、`lan-multicast` 同构）

**移动端流程与 UI**
- `mobile/src/app/onboarding/` — 新增接收目录选择步骤
- `mobile/src/components/transfer-offer-host.tsx` — 接受前的落点校验与引导
- `mobile/src/core/saf-intent.ts` — `canOpenSaveFolder()` 判据随落点治本收敛
- `mobile/src/app/inbox/[itemId].tsx` 与收件箱列表 — 新增「发送到设备」动作
- `mobile/src/stores/share-store.ts` — 承载收件箱来源

**Rust 侧**
- `mobile-core/src/file_access.rs` — publish 分支判据语义收敛（无行为改动）
- `mobile-core/src/file_staging.rs` — 随 `dataDir` 迁移，无代码改动

**Web 端**
- `crates/web/src/file_access.rs` — 新增从 OPFS 注册文件源
- `docs/app/app/inbox/` — 新增发送入口；批量导出

**文档与契约**
- `DESIGN.md` — 新增跨端契约条目（与 `Device Card Contract`、`Node Status Contract` 并列）
- `CLAUDE.md` — 「接收是暂存 → 发布两段」段落需同步落点描述
- `dev-notes/knowledge/rust-backend.md` — `dataDir` 语义变更

**风险面**
- SAF 授权失效（用户清除数据 / 删除目录 / 系统撤销）需探活与重新授权流程
- iOS `Documents` 默认进 iCloud 备份，大文件占用用户 iCloud 空间
