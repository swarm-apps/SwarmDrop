## 1. 提取可测的自动启动判定

- [x] 1.1 新增具名函数 `autoStartNodeIfEnabled()`（落点：`src/stores/network-store.ts` 的
      导出函数区，与既有的 `startNetworkFromStore()` 同处），职责为「读一次 `autoStart`
      偏好 → 为真则调用 `startNetwork()`」，自身不持有状态、重复调用无副作用
- [x] 1.2 函数内不做路由判断、不 await 进任何首屏门禁；失败不额外提示（沿用
      `startNetwork()` 内部的 error 状态与 toast）

## 2. 接进冷启动序列

- [x] 2.1 在 `src/main.tsx` 的启动序列中，于 `.then(() => syncDeviceNameFromBackend())`
      **之后**触发 `autoStartNodeIfEnabled()`，且不进入决定 `setIsLoaded(true)` 的链路
      （落在 `.finally` 内、`setIsLoaded` 之后，以 `void` 调用不阻塞）
- [x] 2.2 在该调用处写明顺序判据的理由（设备名先于节点启动，否则冷启动那一次广播旧名字），
      引用移动端 `mobile-core-store.ts` 的同一条注释

## 3. 删除响应式收敛环

- [x] 3.1 删除 `src/routes/_app.tsx` 中依赖 `networkStatus` 的自动启动 effect
- [x] 3.2 清理该 effect 遗留的 `autoStart` / `networkStatus` / `startNetwork` 订阅
      （三个 selector 与 `useNetworkStore` / `usePreferencesStore` 两个 import 均已无其他
      使用者，一并删除；原位留一条注释指向新落点）
- [x] 3.3 确认 `_app.tsx` 其余三个 effect（传输监听 / 解除配对监听 / 改名监听）不受影响

## 4. 测试

- [x] 4.1 `src/stores/network-store.test.ts`：开关关闭时 `autoStartNodeIfEnabled()`
      不调用 `commands.start`
- [x] 4.2 开关打开时调用一次 `commands.start`
- [x] 4.3 回归锚点：开关打开 → 启动 → `stopNetwork()` → 断言状态停在 `stopped`
      且 `commands.start` 未被再次调用（本 bug 的直接回归测试）
- [x] 4.4 ~~`useNodeRestart.restart()` 期间 `commands.start` 恰好被调用一次~~
      ~~改为源码护栏（在 network-store.test.ts 里 readFileSync + 正则扫 `_app.tsx`）~~
      **最终形态：`scripts/check-node-lifecycle.mjs` + `pnpm check:node-lifecycle`**。
      两次改方案的理由链：① `renderHook` 不渲染 `_app.tsx`，测不到真正的回归源；
      ② 单测里的源码 grep 只扫**一个**文件——同一个收敛环长在 `__root.tsx` 或任意组件里
      都照绿，而注释写得像已经钉住了回归源，**部分覆盖的护栏比没有护栏更糟**；
      ③ 这是跨文件的架构约束，仓里已有 `pnpm check:*` 这套正规机制（clipboard 那条形状
      逐字相同）。新脚本按「`useEffect` 内不许出现启停调用」判定（禁的是**响应式**调用，
      不是组件调用——用户点按钮当然要调），扫 `src/` 与 `docs/app/app` 共 219 个文件，
      已正反向验证：塞回原收敛环会红、还原后绿
- [x] 4.5 确认无渲染 `_app` 布局的测试（全仓无此类用例，无需调整）

## 5. 机器门禁

- [x] 5.1 `pnpm test` 全绿（35 文件 / 255 测试）
- [x] 5.2 `pnpm check:zustand-access` 通过
- [x] 5.3 `pnpm build`（tsc + vite build）通过；另跑 `check:clipboard` /
      `check:shared-view` / `check:landing` 三条均通过
- [x] 5.4 新门禁 `pnpm check:node-lifecycle` 已接进 `package.json`、`CLAUDE.md` 的命令清单
      与 `/dev-workflow` 的门禁清单（否则它只是个没人跑的脚本）

## 6. 真机验证（桌面）

- [x] 6.1 `autoStart: true`（本机既有偏好）下启动应用 → 节点自动启动（日志见 bootstrap 拨号，顶栏徽章「公网可达」）
- [x] 6.2 在上述状态下经 UI 点「停止节点」→ **节点保持停止**。两侧证据：前端徽章稳定在
      「未启动」；后端日志显示全链路关停（`actor stopped` / infra supervisor 退出 /
      presence supervisor 退出 / 事件循环退出）且**此后无任何重启序列**——旧代码在这里
      会立刻出现一整套新的启动日志
- [x] 6.3 停止状态下切到 `/settings` → 仍是「未启动」
- [ ] 6.4 改一项需重启的网络设置 → 点「重启节点」→ 确认重启成功且提示「节点已重启」
- [ ] 6.5 关闭「自动启动节点」→ 重启应用 → 确认节点保持停止

## 7. 收尾

- [x] 7.1 三道关：机器门禁全绿 ✅ · `/simplify` 四个 agent 去重后应用 11 条 ✅ ·
      `/code-review` 进行中
- [x] 7.2 已在 `dev-notes/knowledge/theme-and-styling.md` 新增条目
      「一次性的启动意图不能写成依赖状态的 effect」——含判据（这个 effect 表达的是一次
      动作还是一条不变量）、三端形态对照、以及「幂等门禁会掩盖这类竞态」这条推论
