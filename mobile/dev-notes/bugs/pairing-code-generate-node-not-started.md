# 节点未启动时生成配对码会直接报错

日期：2026-06-28

## 现象

Android dev build 中，在节点未启动的状态打开“添加设备”面板并切到“生成”页时，Metro 日志出现：

```text
[pairing-code] generate failed: [Error: FfiError.NodeNotStarted]
```

用户界面没有明确引导先启动节点，容易表现为配对码区域无内容或生成失败。

## 初步判断

`GenerateTab` 进入时会直接调用 `usePairingCodeStore.ensure()`，而 native 侧生成配对码依赖运行中的移动节点。当前 UI 没有像“附近”页一样先根据 `runtimeState` 展示“节点未启动”的状态。

## 后续修复方向

- `GenerateTab` 读取 `runtimeState`，节点未启动时展示启动提示，不调用 `ensure()`；
- 或者在打开 PairingSheet 前统一检查节点状态，引导用户先启动节点；
- Maestro 可补一条 stopped-node 场景，验证不会在未启动时触发生成错误。
