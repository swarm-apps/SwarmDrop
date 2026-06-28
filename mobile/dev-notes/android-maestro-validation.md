# Android Maestro 验证

## 范围

当前移动端 UI 自动化只要求 Android。iOS 由于 Expo SDK 56 / RN 0.85 Fabric 在 iOS 26 模拟器上的 accessibility tree 不稳定，暂不作为 Maestro 必过门禁。

Maestro 负责验证单设备可重复的 UI surface：

- 首次进入与三主 tab；
- Devices / Inbox / Activity / Settings 导航；
- Devices、Inbox、Activity 空态；
- 配对、节点控制、设备策略、网络发现设置；
- Inbox 详情和删除确认在已有 fixture 状态下的可见性。

真实跨设备吞吐、配对成功率、传输恢复和接收 offer 的完整链路仍属于人工或专项联调。没有安全 fixture 时，不应为了自动化测试伪造生产 store 状态。

## 本地运行

1. 启动 Android 模拟器或连接 Android 设备，确认 `adb devices` 能看到目标设备。
2. 启动开发包并保持 Metro 运行：

```bash
pnpm android
```

3. 另开终端运行 smoke flow。推荐先跑 onboarding 初始化，再按 surface 跑剩余 flow：

```bash
mkdir -p build/maestro-results build/maestro-debug

for flow in \
  .maestro/smoke/onboarding.yaml \
  .maestro/smoke/empty-states.yaml \
  .maestro/smoke/mobile-foundation.yaml \
  .maestro/smoke/inbox.yaml \
  .maestro/smoke/inbox-delete-confirmation.yaml \
  .maestro/smoke/device-policy.yaml \
  .maestro/smoke/network-settings.yaml
do
  name="$(basename "$flow" .yaml)"
  /Users/yexiyue/.maestro/bin/maestro --device emulator-5554 test \
    --test-output-dir "build/maestro-results/$name" \
    --debug-output "build/maestro-debug/$name" \
    "$flow"
done
```

如果只需要快速验证壳层导航，可以单独运行：

```bash
/Users/yexiyue/.maestro/bin/maestro --device emulator-5554 test .maestro/smoke/mobile-foundation.yaml
```

如果当前设备 ID 不是 `emulator-5554`，用 `adb devices` 返回的设备 ID 替换 `--device` 参数；只有一个设备时也可以省略 `--device`。

## 产物

- `build/maestro-results/<flow>/`：测试报告和步骤产物。
- `build/maestro-debug/<flow>/`：Maestro 调试日志。
- flow 内的 `takeScreenshot` 会在当前工作目录生成 PNG；提交前删除这些截图，避免把运行产物混入代码提交。
- Metro / Android logcat 中的既有 warning（如 `@noble/hashes` exports、require cycle、SafeAreaView deprecation）不直接代表本 flow 失败；以 Maestro 断言结果和 app 崩溃日志为准。

## 约束

- 所有稳定 flow 优先使用 `testID`，不要依赖可本地化文案。
- 坐标点击只用于关闭开发 warning 或临时视觉采样，不作为核心断言手段。
- receive offer 自动化需要确定性 fixture 后再加入 smoke；在此之前只保留 dialog / 按钮 testID，避免把不真实状态写进生产代码。
