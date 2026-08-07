# research —— 方案调研与可行性验证

**放什么**：还没落地的方案的调研、spike 验证结论、技术选型判断。每篇都带**决策状态**。

**不放什么**：已经落地的东西。那些属于 [`knowledge/`](../knowledge/)（开发时必读的
实践与踩坑）或 [`architecture/`](../architecture/)。

## 为什么和 knowledge/ 分开

`knowledge/` 是**开发前必读**的——`/dev-workflow` 会把它加载进上下文，里面的每条都被当作
「现行架构的事实」。调研文档混进去会造成两类误导：

1. 把「评估中的方案」读成「已有的能力」
2. 调研的时效性比实践记录短得多——上游一个 PR 合并，整篇结论就可能翻转

历史上 `knowledge/` 已经吃过这个亏：`libp2p-wasm.md`、`iroh-migration.md`、
`storage-abstraction.md` 三篇本质是调研快照，只能靠开头挂「已决策 / 未决策」横幅来救。
新的调研一律进这里。**调研的结论一旦落地成代码，再把「实践部分」提炼进 `knowledge/`。**

## 篇目

| 主题 | 状态 | 一句话 |
|---|---|---|
| [自研 WebRTC transport：浏览器直连 NAT 后设备](2026-07-webrtc-native-ice.md) | 🟢 已决策采纳 | webrtc-rs 0.20 有完整 ICE 能力，地基已实测；驱动理由是能力建设与生态缺口，非投入产出比 |
| [Web 应用区 i18n 选型](2026-08-web-app-i18n.md) | 🟡 已给结论待实施 | Lingui + 运行时切换、不引 `[lang]` 段；macro 在 Next 16 Turbopack 下已实测可编译（#102） |
| [Rust 侧的中文串：它是不是 i18n 问题](2026-08-rust-side-user-strings.md) | 🟢 已决策并落地 | 1466 处里到得了用户的只有 3 条通道；推荐判别码化而非 Rust 侧 i18n。附带发现一个真 bug（文件名参与错误匹配） |
| [`docs/` 依赖升级评估](2026-08-docs-deps-upgrade.md) | 🟡 部分落地 | Next 16.3 已升（它是 dev 吃光内存把机器搞重启的正解）；fumadocs 换搜索引擎 + lucide v1 删品牌图标要一起改，TS 7 等 7.1 |
| [三端日志：给用户一份能交出来的现场](2026-08-logging.md) | 🟡 待决策 | **移动端连 subscriber 都没有、日志根本不产生**，且 Android 上 stdout 进 /dev/null（`log.redirect-stdio` 只在 Dalvik 有效）；桌面只到 stdout。官方 `tauri-plugin-log` 只吃 `log` crate 而本仓全是 `tracing` → 文件层用 `tracing-appender` 三端共用，平台层各挂各的。建议先做移动端 |

状态图例：🟢 已决策采纳 · 🟡 验证中 / 待决策 · 🔴 已否决（保留论证）
