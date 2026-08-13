# file-thumbnail Specification

## Purpose
TBD - created by archiving change unify-file-browser. Update Purpose after archive.
## Requirements
### Requirement: 缩略图的生成判定 SHALL 由共享契约统一

「一个文件该不该有缩略图」MUST 由 `packages/shared-view/src/file-browser/thumbnail.ts` 的纯函数判定，三端 MUST 使用同一份判据：文件类型属于图片扩展名集合（`media-type` 的唯一来源），且文件大小不超过尺寸门槛，且该项存在可用的取图源。

不满足任一条时 MUST 渲染文件类型图标，MUST NOT 留白或显示破图。

#### Scenario: 超过尺寸门槛的图片不生成缩略图

- **WHEN** 网格视图渲染一个大小超过门槛的图片文件
- **THEN** 系统 MUST NOT 触发解码，MUST 直接渲染该类型的文件图标

#### Scenario: 非图片类型不生成缩略图

- **WHEN** 网格视图渲染一个压缩包或文档
- **THEN** 系统 MUST 渲染类型图标，MUST NOT 尝试取图源

### Requirement: 缩略图 SHALL 缩放后缓存，不常驻原图

生成管线 MUST 把原图解码后缩放到不超过约定长边（320 px）并重编码，缓存的 MUST 是缩放产物而非原图。缓存 MUST 有容量上限，条目被淘汰或组件卸载时 MUST 释放其 object URL。

#### Scenario: 一屏图片的常驻内存与原图大小无关

- **WHEN** 用户在网格视图滚动浏览一批数 MB 级的图片
- **THEN** 缓存中常驻的 MUST 只有缩放产物，任一原图 MUST NOT 在其缩略图生成完成后继续常驻

#### Scenario: 淘汰与卸载都释放 object URL

- **WHEN** 缓存条目被容量上限挤出，或使用该缩略图的组件卸载
- **THEN** 对应的 object URL MUST 被 revoke

### Requirement: 缩略图 SHALL 按需触发并限制并发

缩略图 MUST 只对进入视口的条目触发生成，MUST NOT 在列表挂载时对全部条目一次性触发。解码 MUST 受并发上限约束。

#### Scenario: 视口外的条目不解码

- **WHEN** 一个包含数十个图片的会话以网格视图打开
- **THEN** 只有当前可见的条目 MUST 触发生成，其余 MUST 在进入视口后才触发

### Requirement: 各端 SHALL 各自提供取图源，管线保持同构

取图源函数 MUST 由各端注入，管线本身 MUST NOT 认识任何一端的存储形态：

- 桌面：由本地路径经 Tauri 的资源协议解析
- 移动：由 `file://` 形式的本地 URI 解析
- Web：由 OPFS 相对路径经 `crates/web` 导出的文件句柄接口解析为 `File`

Web 侧的接口 MUST 返回惰性的 `File` 引用而非已读入内存的字节，且下载路径（object URL 导出）MUST 复用同一个句柄获取层。

#### Scenario: Web 端从 OPFS 取图

- **WHEN** Web 网格视图需要一个已接收图片的缩略图
- **THEN** 系统 MUST 经 OPFS 句柄拿到 `File` 后交给管线，MUST NOT 先生成 object URL 再回读

#### Scenario: 三端产出规格一致

- **WHEN** 同一个图片文件分别在三端生成缩略图
- **THEN** 判定、目标长边与缓存 key MUST 来自同一份共享契约

### Requirement: 取图源不可用时 SHALL 优雅降级

取图源缺失、存储不可用（如非 secure origin 下 OPFS 不存在）、解码失败三种情况 MUST 都回落到文件类型图标，MUST NOT 阻塞列表渲染，MUST NOT 把错误抛到用户面前。

#### Scenario: 非 secure origin 下的 Web 网格视图

- **WHEN** Web 应用运行在 OPFS 不可用的源上
- **THEN** 网格视图 MUST 正常渲染并全部显示类型图标

#### Scenario: 解码失败的单个条目不影响其他条目

- **WHEN** 某个图片文件已损坏导致解码抛错
- **THEN** 该条目 MUST 回落到类型图标，同一列表中其他条目的缩略图 MUST 照常生成

