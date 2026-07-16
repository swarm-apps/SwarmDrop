# 配对 BottomSheet 初始高度只露出 Tab

日期：2026-06-28

## 现象

Android 端在设备页点击“添加设备”后，配对 BottomSheet 已经打开，但首屏只露出“附近 / 生成 / 输入”三个 tab，附近设备列表、配对码内容或 OTP 输入区域被压在屏幕底部之外。用户需要额外手势或会误以为面板没有弹出。

## 原因

`PairingSheet` 使用 `enableDynamicSizing`，但外层是 `BottomSheetView`，实际可滚动内容藏在 tab 内容里。动态测高阶段只稳定测到了 tab header 附近的高度，没有给当前 tab 内容预留足够空间。

## 修复

将配对面板改为固定 snap point，并让整个 tab 区域位于同一个 `BottomSheetScrollView` 内：

- 初始高度使用 `58%`，保证附近设备或输入区首屏可见；
- 最大高度使用 `84%`，给键盘和较长设备列表留空间；
- 关闭这块的动态测高，避免 tab 内容切换时再次测出过小高度；
- 附近设备列表改成普通布局，由外层 BottomSheet ScrollView 统一滚动。

后续如果某个 BottomSheet 的主要内容藏在 tab、条件渲染或内部 scroll view 中，应优先使用明确 snap point，而不是依赖动态测高。
