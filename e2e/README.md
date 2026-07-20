# E2E 总览

两端各有一套 E2E，都基于 WebdriverIO，但**驱动层不同、位置不同**：

| | 位置 | 驱动 | 跑什么 |
|---|---|---|---|
| 桌面 | `e2e/desktop/` | `@wdio/tauri-service` → 真实 Tauri 二进制 | `test/specs/app-launch.native.e2e.ts` |
| 移动 | `mobile/e2e/webdriver/` | Appium + XCUITest → iOS 模拟器/真机 | `test/specs/{onboarding,file-browser,accept-transfer}.e2e.ts` |

## 为什么不合并到一起

看着像该合（都是 WDIO，还有 6 个同版本依赖各装一份），但评估后**刻意保持分开**：

- **各自贴近被测对象**。`e2e/desktop` 挨着 Tauri 壳，`mobile/e2e` 挨着 RN app —— 这不是历史包袱。
- **合并消除不掉跨目录**。移动 E2E 需要 Metro dev server，那个必须在 `mobile/` 里起（见下方 `record:transfer`）。合并只改变代码放哪，不改变运行时仍要跨目录。
- **移动 E2E 的 TS 环境是继承来的**：`mobile/e2e/webdriver/tsconfig.json` `extends ../../tsconfig.json`（Expo 的配置）。挪走就得自己重配一份。
- 收益（省点磁盘）小于成本（重配 tsconfig + 改 scripts + 动两个 lockfile + 冒险打断已验证的双端录制链路）。

`mobile/` 是独立 pnpm workspace，所以移动 E2E 的依赖装在 `mobile/package.json`；桌面 E2E 的装在 `e2e/desktop/package.json`。两边互不可见，这是 workspace 边界决定的，不是疏忽。

## 入口

```bash
# 桌面 E2E（native 模式驱动真实二进制）
pnpm --dir e2e/desktop wdio

# 移动 E2E（需先起 Appium / 装好 xcuitest driver）
pnpm --dir mobile e2e:ios
pnpm --dir mobile e2e:ios:driver     # 查已装的 driver
pnpm --dir mobile e2e:ios:doctor     # 环境体检
```

**桌面 native 二进制必须用 `pnpm tauri build --debug --no-bundle` 构建** —— 裸 `cargo build` 出来的会因为 `devUrl` 指向没启动的 Vite 而白屏。详见 `dev-notes/knowledge/toolchain.md`。

## Demo 素材录制

与常规 E2E 分开：常规是 `test/specs/**/*.e2e.ts`，录制是 `test/specs/demo/*.demo.ts`。

```bash
# 桌面单场景 / 套装
pnpm --dir e2e/desktop record desktop-home
pnpm --dir e2e/desktop record desktop-suite

# 双端真实传输（外层 orchestrator）
pnpm --dir e2e/desktop record:transfer

# 移动模拟器单独录屏
pnpm --dir e2e/desktop record:mobile android
```

`record:transfer` 是唯一跨两端的入口：它在 `mobile/` 里起 Metro，然后并行跑桌面 WDIO demo flow 和移动端 `e2e:ios`，两端都 ready 后才启动 OBS 录制。**这就是「合并 E2E 目录也省不掉跨目录」的那条链路** —— 它靠 `cwd` 切换工作，不需要两套代码住在一起。

## 延伸阅读

- `e2e/desktop/demo-asset-plan.md` — 素材计划与三条轨道
- `e2e/desktop/demo-postproduction-design.md` — 后期与 timeline schema
- `mobile/e2e/webdriver/README.md` — Appium/XCUITest 录屏、端口、真机与模拟器差异
- `dev-notes/knowledge/demo-recording.md` — 录制平台选择与产物约定
- `dev-notes/blogs/desktop-webdriver-e2e.md` — 桌面 WDIO 的已知坑
