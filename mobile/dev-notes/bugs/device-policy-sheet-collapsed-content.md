# 设备策略 BottomSheet 没有弹出

日期：2026-06-28

## 现象

在设备详情页点击“策略设置”后，策略面板没有稳定出现在屏幕上。用户看到的效果接近“没有弹出”，实际可能是面板被动态测高成过小高度，或首帧内容高度没有被 BottomSheet 正确采集。

## 原因

设备策略面板沿用了 `enableDynamicSizing` + `BottomSheetView` 的占位实现。策略编辑器后续会包含条件内容、开关组和确认操作，动态测高很容易在首次 present 时拿到不完整内容高度。项目里配对面板已经出现过同类问题，不能继续依赖动态测高承载复杂内容。

## 修复

设备策略面板改成固定 snap point，并把内容放进 `BottomSheetScrollView`：

- 初始高度使用 `72%`，打开后策略选项和主要按钮可见；
- 最大高度使用 `90%`，给更多策略项、确认操作和小屏幕留空间；
- 关闭 `enableDynamicSizing`，避免首帧测高过小；
- 主要操作放到 `@gorhom/bottom-sheet` 的 `footerComponent`，而不是普通子视图里；
- 保留 `testID="device-policy-sheet"` 和 `device-policy-save-button`，供 Android Maestro 验证。

后续复杂 BottomSheet 只要包含条件渲染、内部表单、tab 或较多操作按钮，优先使用明确 snap point。
如果需要固定底部操作栏，应使用 `footerComponent`/`BottomSheetFooter`，普通 `View` 在
`BottomSheetModal` 子树里不会天然获得受约束高度，footer 仍可能被排到滚动内容之后。
