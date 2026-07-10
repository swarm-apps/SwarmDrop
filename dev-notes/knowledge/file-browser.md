# 统一文件浏览组件

应用内所有文件集合统一通过 `src/components/file-browser/` 展示，页面不直接使用树引擎，也不自行实现文件卡片。

## 数据与视图

- 页面先使用 adapter 把发送扫描文件、Offer、传输投影或收件箱文件转换为扁平 `FileBrowserItem[]`。
- `FileBrowser` 支持 `tree` 与 `grid`；树形目录由扁平相对路径派生，网格按行虚拟化。
- `send`、`inbox`、`transfer` 的视图偏好分别持久化。默认依次为 tree、grid、tree；Offer 与传输详情共用 transfer 偏好。
- 文件状态统一为 `idle`、`waiting`、`transferring`、`completed`、`error`、`missing`。

## 操作与安全边界

- 删除、打开、定位、重试都通过 `actions` 显式传入；组件不根据页面或 mode 猜测业务行为。
- 目录删除目标使用带尾斜杠的相对路径前缀。
- 组件不会从 `localPath` 自行生成 WebView URL。只有收件箱 adapter 接收到调用方显式提供的 `previewUrl` 时才加载缩略图；发送任意本地文件默认只显示类型图标，不能为预览扩大 Tauri asset scope。

## 布局约束

外层页面需要维持 `flex min-h-0` 高度链。FileBrowser 的 header 与页面命令栏固定，树形或网格内容区独立滚动。文件夹展开态不使用常驻高亮背景，只改变 chevron 与文件夹图标。

发送进度页的 `TaskToolbar` 和 `CommandDock` 必须放在中间滚动区之外。成功摘要与文件明细共同位于唯一的 `overflow-auto` 内容区，文件明细面板使用稳定高度（小屏 360px、桌面最高 440px），避免成功摘要变高时把文件列表压缩成很小的区域。

任务页统一通过 `TaskContent.footer` 固定底部 `CommandDock`。发送选择、配对输入和配对码页面都使用这套结构；不要再把命令栏作为 `TaskContent` 的普通 children。快捷发送已自行保持中间面板伸缩与底部命令栏固定。

接收 Offer 弹窗同样使用完整 FileBrowser，支持 tree / grid 并复用 transfer 视图偏好。弹窗采用宽版紧凑布局和 280–420px 响应式文件区；不要因为弹窗空间有限退回私有列表或禁用网格，而应通过弹窗宽度、头部密度和稳定文件区解决拥挤。
