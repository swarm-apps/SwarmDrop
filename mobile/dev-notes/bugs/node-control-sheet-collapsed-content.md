# 节点控制 BottomSheet 没有稳定弹出

## 现象

Android 端在设备页点击“管理”后，节点控制 BottomSheet 没有稳定出现在屏幕上。用户看到的效果接近“没有弹出”，尤其是在面板包含更多网络状态行后更容易复现。

## 原因

`NodeControlSheet` 原本使用 `enableDynamicSizing`，内容又会根据节点运行状态切换。网络发现同步后，运行态新增了发现方式、候选节点、LAN Helper、中继来源和本机 Helper 等多行信息，首次 present 时动态测高容易拿到过小或不完整的内容高度。项目里配对面板和设备策略面板已经出现过同类问题。

## 修复

将节点控制面板改为固定 snap point，并保留 `BottomSheetScrollView` 承载长内容：

- 使用 `["62%", "88%"]` 明确面板高度；
- 关闭动态测高；
- 给设备页入口、面板内容和取消按钮补稳定 testID；
- 在 mobile foundation Maestro 冒烟流中覆盖“点击管理后 sheet 可见”的回归验证。

后续包含条件渲染、运行状态详情或多行操作信息的 BottomSheet，优先使用明确 snap point。
