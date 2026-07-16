# Demo 录制后期处理设计

> 状态：设计稿
>
> 目标：把可重复的 E2E 演示录制，变成可重复生成的产品 Demo 成片。原始录制负责事实，Remotion
> 负责剪辑、字幕、镜头和版本输出。

## 1. 背景与结论

当前桌面端录制由 `record-desktop-demo.mjs` 编排：WDIO 进入稳定页面后写入 `ready` 文件，编排器启动
OBS、等待短暂延迟，再写入 `go` 文件放行演示操作。原片会复制到
`build/desktop-recordings/raw/`，运行元数据写入 `build/desktop-recordings/manifests/`。

这套机制已经解决了“视频包含应用启动等待”的问题，但它只产出 MP4，没有告诉后期工具：

- 哪一帧对应哪一次点击；
- 该放哪句说明文字；
- 需要把镜头放大到哪里；
- 哪些等待应当剪掉。

因此不应把 Remotion 当作另一个录制器，而应把它作为 **E2E Demo 的声明式后期层**。WDIO 额外产出
一份事件时间线，Remotion 消费“原片 + 时间线”导出官网、文档和短视频版本。

```text
WDIO Demo Spec ──┐
                ├─ 原片 MP4 / MOV ───────┐
                └─ 事件时间线 JSON ──────┼─ Remotion Composition ── MP4 / GIF / 静帧
OBS / 模拟器 ────────────────────────────┘
```

## 2. 目标与非目标

### 目标

- 录制后自动裁掉 `go` 之前的启动等待和流程中的无效停顿。
- 对关键点击显示鼠标、光圈和跟随放大，而不是依赖肉眼猜测坐标。
- 字幕由演示脚本声明，和操作语义一致、可中英双语、可随 UI 改版一同评审。
- 一份 Demo 可以导出 16:9 官网版、9:16 短视频版、文档 GIF / 静帧。
- 不提交原始长录屏；成片和编辑代码可复现。

### 非目标

- 不实现 Premiere / Final Cut 式的通用手工时间线编辑器。
- 第一阶段不做鼠标轨迹识别、自动主体追踪或从无声录屏自动生成字幕。
- 不让 Remotion 参与 E2E 成功判定；测试仍只对真实应用状态负责。

## 3. 时间线的唯一事实来源

### 3.1 录制零点

当前顺序为：

```text
WDIO 写 ready → OBS StartRecord → 等待 WDIO_RECORDING_DELAY_MS → 写 go → Demo 开始操作
```

后期时间线的 `0ms` 定义为 Demo 观察到 `go` 的时刻，而非 OBS 调用 `StartRecord` 的时刻。这样所有
事件都只和 WDIO 自己的时钟有关。

原片在 Remotion 中先裁掉 `sourceTrimMs`（初始等于 `WDIO_RECORDING_DELAY_MS`），再叠加事件。该值
必须写进 manifest，不能在 Composition 中猜一个 250ms 常量。

第一阶段允许人工以一帧为单位微调 `sourceTrimMs`；确认稳定后，再把该校准值固化为录制预设。

### 3.2 事件时间线

每次录制新增以下临时产物，和原片使用同一个时间戳：

```text
e2e/desktop/build/desktop-recordings/
├── raw/<stamp>-send-file.mkv
├── manifests/<stamp>-send-file.json       # 已有
└── timelines/<stamp>-send-file.json       # 新增，忽略提交
```

示例：

```json
{
  "schemaVersion": 1,
  "demo": "send-file",
  "source": {
    "path": "raw/2026-07-14T03-20-00-000Z-send-file.mkv",
    "sourceTrimMs": 250,
    "contentRect": { "x": 0, "y": 0, "width": 1440, "height": 900 }
  },
  "events": [
    {
      "atMs": 420,
      "type": "caption",
      "text": "选择一台在线设备"
    },
    {
      "atMs": 780,
      "type": "click",
      "target": { "x": 1068, "y": 542, "width": 132, "height": 40 },
      "caption": "点击发送"
    },
    {
      "atMs": 1320,
      "type": "focus",
      "target": { "x": 732, "y": 448, "width": 530, "height": 310 },
      "scale": 1.28,
      "holdMs": 1400
    }
  ]
}
```

`atMs` 始终相对 `go`。`target` 使用应用内容区坐标，不是屏幕绝对坐标。

### 3.3 坐标映射是硬门槛

WDIO 元素矩形默认相对 webview 视口，而 OBS 有可能采集整个屏幕、窗口或裁剪后的来源。若直接把元素
坐标画到视频上，光圈会漂移。

所以每条时间线必须保存 `contentRect`：它描述应用内容区在原视频中的像素位置。渲染时统一换算：

```text
videoX = contentRect.x + elementX × contentRect.width / viewport.width
videoY = contentRect.y + elementY × contentRect.height / viewport.height
```

首期固定 OBS 为只采集 Tauri 内容窗口，并在录制预设中固定分辨率；变更 OBS 场景或窗口缩放后，必须重新
校准 `contentRect`。没有有效 `contentRect` 的时间线不得启用点击跟随。

## 4. E2E 注解 API

在 `test/specs/demo/helpers.ts` 增加一层演示语义 API，测试不直接写 JSON。

```ts
await demoStep({ caption: "选择一台在线设备" });
await demoClick(sendAction, { caption: "点击发送", focus: true });
await demoFocus($('[data-testid="file-drop-zone"]'), {
  caption: "拖入要传输的文件",
  scale: 1.28,
  holdMs: 1400,
});
```

约束：

- `demoClick` 在实际 `element.click()` 前读取元素矩形，并以 `go` 后的单调时间写入事件。
- `demoFocus` 只表达镜头意图，不改变应用状态。
- `demoStep` 只写字幕 / 章节，不伪造点击。
- 非录制模式下这些函数是无副作用空操作，普通 E2E 不新增文件、不变慢。
- 测试继续使用 `data-testid` 定位，后期层不依赖可翻译文案或 CSS 选择器。

## 5. Remotion 后期工程设计

`video/` 保持独立 pnpm workspace。源片不进入 Git；选择用于成片的短片由本地准备命令复制或软链接到
`video/public/demos/`，该目录也忽略提交。

```text
video/
├── public/demos/                     # 本地原片，忽略提交
├── src/demos/
│   ├── data/
│   │   └── send-file.ts              # 可提交的编辑计划与字幕文案
│   ├── DesktopDemo.tsx               # 16:9 成片
│   ├── MobileDemo.tsx                # 9:16 变体
│   └── components/
│       ├── ClickSpotlight.tsx
│       ├── DemoCaption.tsx
│       ├── FocusCamera.tsx
│       └── DemoSource.tsx
└── scripts/
    └── prepare-demo-assets.mjs       # 校验 / 导入原片和时间线
```

### 5.1 剪辑

`DemoSource` 使用 Remotion 的视频组件播放原片；每段采用帧范围而非实时等待。编辑计划以秒或毫秒描述，
在 Composition 中统一换算为帧。

- 删除 `sourceTrimMs` 之前的画面。
- 每个事件前保留约 300–500ms 建立上下文，点击后保留 700–1400ms 让观众阅读结果。
- 不在原片中加速真实加载过程来掩盖性能问题；要剪掉就直接剪掉。
- 16:9 默认保留完整应用窗口；9:16 通过 `FocusCamera` 取事件目标附近区域，不能把重要 UI 截断。

### 5.2 点击跟随

对 `click` / `focus` 事件生成三个独立层：

1. `FocusCamera`：在 8–12 帧内平滑移动并缩放到目标区域；
2. `ClickSpotlight`：目标中心出现 18–24 帧的鼠标指针、光圈和波纹；
3. `DemoCaption`：放在安全区内的短句说明。

镜头缩放和光圈都只由 `useCurrentFrame()`、`interpolate()` 驱动，禁止 CSS animation / transition。每个
事件的画面必须能在任意帧单独渲染并得到确定结果。

### 5.3 字幕与旁白

首期使用脚本字幕：`caption` 是受评审的产品文案，不从无声录屏猜测内容。字幕规则：

- 一次只展示一个动作或结果，中文不超过两行；
- 仅描述用户可见行为，例如“点击发送”“选择要传输的文件”；
- 默认不显示姓名、邮箱、真实文件名、绝对路径、设备别名或其他隐私信息；
- 需要英语版本时，使用同一事件 ID 的翻译表，不修改时间点。

后续若加入旁白，可由语音转写生成字幕初稿，但仍以编辑计划中的人工文案为准。

## 6. 隐私与素材治理

- E2E fixture 必须使用通用设备名、通用文件名和虚构内容；禁止把开发者姓名写进可录制界面或字幕。
- 原片、导出的中间片段、自动生成的事件时间线都位于 `build/` 或 `video/public/demos/`，默认忽略提交。
- 只有经检查的最终官网成片、封面图和不含隐私的编辑计划可提交。
- 发布前至少检查首帧、每个点击帧、每个字幕帧和末帧；发现用户名、路径或通知弹窗即丢弃原片重录。

## 7. 分阶段实施

### Phase A：事件采集

1. 在录制 manifest 加入 `sourceTrimMs`、源视频尺寸和内容区矩形。
2. 在 demo helpers 实现 `demoStep`、`demoClick`、`demoFocus`。
3. 先为 `send-file.demo.ts` 产出一份时间线，验证时钟与坐标。

验收：同一次录制可生成原片、manifest、timeline；关闭录制模式后不产生产物。

### Phase B：桌面后期 Composition

1. 增加 `DesktopDemo`、点击光圈、镜头缩放和脚本字幕组件。
2. 导入 Phase A 的 `send-file` 原片，输出 16:9 MP4 和一张封面。
3. 对每个事件帧做静帧检查，确认字幕、光圈和实际元素对齐。

验收：删掉原片前等待，点击位置偏差不超过 8px，字幕不溢出安全区。

### Phase C：复用与多端

1. 把 `desktop-home`、`inbox`、`lan-transfer` 纳入相同协议。
2. 新增 9:16 模板；移动端独立录屏也复用相同 timeline schema。
3. 根据渠道导出官网 MP4、文档 GIF、社媒短视频。

验收：同一份事件数据可导出至少两种纵横比，不需要手工重做字幕和点击特效。

## 8. 当前实现映射

| 现有位置 | 职责 | 后续改动 |
| --- | --- | --- |
| `e2e/desktop/scripts/record-desktop-demo.mjs` | OBS 编排、原片复制、manifest | 增加时间线目录和同步元数据 |
| `e2e/desktop/test/specs/demo/helpers.ts` | ready/go 门控、截图、暂停 | 增加演示事件收集 API |
| `e2e/desktop/test/specs/demo/*.demo.ts` | 真实交互流程 | 用语义化 `demoStep` / `demoClick` 标记镜头 |
| `video/` | 官网 Hero 成片 | 新增可消费 Demo 原片的 Composition |
| `docs/public/hero/` | 静态网站最终素材 | 只放审核后的官网成片，不放原片 |

## 9. 决策

先实现“事件驱动后期”，不实现自动识别点击或自动字幕。前者利用已有 WDIO 的确定性，能让每次重录都稳定
复现；后者看似省事，但对 Tauri 窗口、不同 OBS 裁剪和无声素材不可靠，反而会增加人工校对成本。
