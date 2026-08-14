# 联调验证记录

## 桌面与 Web 的真实配对及文本投递

验证日期：2026-08-14。

前置条件：桌面端以 `pnpm tauri dev` 启动，Web 端以 `pnpm --dir docs dev -- --port 3001` 启动；两端先等到网络状态显示可达。

1. 在桌面端生成一条非仅局域网的单次配对邀请。
2. 在 Web 的「设备 → 配对」粘贴邀请并确认。
3. 在桌面全局配对请求面板选择「接受配对」。
4. 刷新 Web 设备页，确认桌面设备出现在已配对列表，且两端均显示在线的局域网连接。
5. 在 Web 的设备发送页切换到「文本」，输入一段 ASCII 文本并发送。
6. 确认桌面端全局「接收文本」对话框展示来源设备和文本正文；选择「接收」。
7. 打开桌面收件箱，确认新记录显示文本标题、来源、字节数和正文，并出现「收到来自 … 的文本」非阻塞反馈。

实测结果：步骤 1–7 均通过。发送端在投递后保留「编辑后重发」入口，收件端把确认后的文本持久化为收件箱记录。

## 自动化界面复核

- Tauri MCP 的无障碍快照确认桌面收件箱显示本次文本的标题、来源 `Chrome`、大小 `30 B` 和正文；截图保存为 `%TEMP%/swarmdrop-desktop-text-inbox-regression.png`。
- 浏览器自动化确认 Web 发送页的文件/文本分段控件、`64 KiB` 计数、粘贴/清空和重发入口，以及设置页的「允许通知」用户手势入口；截图保存为 `%TEMP%/swarmdrop-web-text-notification-regression.png`。
- 原生系统通知的实际弹出无法由当前 Tauri 驱动捕获；对应焦点抑制、后台发布与正文隐私约束已由可注入 publisher 的桌面 fake 覆盖，并可成功编译为测试二进制。通知失败不影响投递由 transfer 领域测试覆盖。
- `cargo test -p swarmdrop-host notification` 通过 3 项测试：聚焦窗口抑制通知、未聚焦时仅发送通用文本提醒，以及发布失败向调用方报告。桌面外壳只保留 Tauri 通知适配；焦点门控和 fake 位于可独立执行的 `swarmdrop-host` crate。
- `pnpm test -- src/routes/_app/send/index.lazy.text-delivery.test.tsx src/components/inbox/text-delivery-attention-host.test.tsx` 通过 7 项测试，覆盖桌面文本粘贴发送、64 KiB 限制、重试、确认/拒绝和失焦后的收件箱定位。
- `pnpm --dir docs test` 通过 12 个测试文件、92 项测试，包含 Web 文本输入、粘贴、清空、超限与重试；`pnpm --dir mobile exec tsc --noEmit --project e2e/webdriver/tsconfig.json` 通过，新增了移动端配对后发送文本的真实设备编排脚本。

## 验证环境边界

- 已新增 `mobile/e2e/webdriver/test/specs/accept-text-delivery.e2e.ts`：真机编排在配对完成后从桌面发送文本，脚本确认移动端出现文本确认框并接受；确认框的接收/拒绝按钮均有稳定的 `testID`。该场景已通过专用 TypeScript 检查。
- 开发模式中的前端热更新会使正在等待的配对 IPC 回调失效，导致 Web 对话框短暂持续显示「配对中」；刷新后会从持久化状态恢复为已配对。这不影响已完成的配对，但正式回归应避免在配对过程中改动前端文件。
- 当前环境没有 Android/iOS 模拟器或实体设备，因此移动端的原生视觉与触摸回归仍需在设备可用时执行。
