## Context

`mobile-core` 目前没有任何 `tracing_subscriber` 初始化，`Cargo.toml` 里只有 `tracing` 门面。
没有订阅器，`crates/*` 里所有 tracing 宏都是空操作——移动端的日志不是「难以取得」，是**从未产生**。

约束条件：

- **移动端用户无法从终端启动应用**，所以文件日志加应用内导出是唯一途径，不像桌面端还能让
  能折腾的用户从终端跑。
- **Android 上 stdout/stderr 进 `/dev/null`**。`log.redirect-stdio` 只在 Dalvik（4.4 及更早）
  有效，ART（5.0+）不支持，必须走 NDK 的原生日志 API。
- **wasm 门禁不能变红**。新依赖若无条件加进共享 crate，`./scripts/check-wasm.sh` 会挂。
- `crates/*` 由三端共用，本次**不得改动共享 crate 的日志调用点**——它们已经在正确地发
  tracing 事件，缺的只是订阅端。

同类先例：Mozilla application-services 同样是 uniffi + 移动端，做法是用各平台原生日志设施、
提供显式启用开关（`RustLog.enable()`）、并支持导出日志文件。本设计与之同构。

## Goals / Non-Goals

**Goals:**

- 让 `crates/*` 既有的 tracing 事件在移动端真正产生输出。
- 开发者能用平台原生工具（`adb logcat` / Console.app）实时看日志。
- 终端用户能自行导出日志文件交给我们。
- 文件层的写法可被后续桌面端改造直接复用。

**Non-Goals:**

- 桌面端与浏览器端改造。桌面端有独立选型结论（见 `dev-notes/research/2026-08-logging.md`），
  浏览器端设置页已有事件日志面板。
- 远程日志上报 / 崩溃收集服务。本项目无遥测，这条不改。
- 结构化日志检索、日志内 UI 查看器。本次只到「能导出」。
- 改动 `crates/*` 中任何现有日志调用点。

## Decisions

### D1：订阅端用 `tracing-subscriber`，不引入 `log` crate 生态

本仓从 `crates/*` 到宿主全部使用 `tracing` 且用了 span。

- **备选**：`android_logger` + `tracing-log` 反向桥接。**否决** —— 该方向别扭（生态里成熟的是
  反过来的 `LogTracer`），且会把 span 上下文压平成一行文本，白丢结构化信息。
- 为一个 writer 引入第二套日志生态不划算。

### D2：iOS 用现成 crate，Android 自己写 writer

| target | 方案 | 理由 |
|---|---|---|
| iOS | `tracing-oslog` | 输出到 os_log，Console.app 与 Xcode 均可见。2.28M 下载、2025-05 仍在更新，可依赖 |
| Android | **自实现 `MakeWriter`** | 见下 |

**Android 侧的现成 crate 都停更了**（实施期核实，推翻了本文件初版的选型）：

| crate | 总下载 | 最近更新 |
|---|---|---|
| `tracing-logcat` | 168,550 | 2024-07（停更约 2 年） |
| `tracing-android` | 675,160 | 2022-01（停更约 4 年） |

- **备选**：直接用 `tracing-logcat` 0.1.0。**否决** —— 依赖一个停更两年的 0.1.0 crate，
  将来 `tracing-subscriber` 升级很可能编不过，届时仍要自己接管，只是把成本推迟了。

这一层非常薄：全部工作就是把格式化后的字节交给 NDK 的 `__android_log_write`。
用 `extern "C"` 声明即可（Android 上 liblog 默认链接），无需 `ndk-sys` 之类的额外依赖。
自实现约 50 行，换来零停更风险与完全可控——与本仓自研 `crates/webrtc-p2p` 的判断一致。
代价是需要自己处理级别映射与 tag 约束，故必须配一条测试。

平台依赖 MUST 写成 `[target.'cfg(target_os = "ios")'.dependencies]` 形式，只落在 `mobile-core`，
不进任何共享 crate——这是 wasm 门禁不变红的前提。

### D3：日志目录由宿主传入，Rust 侧不猜平台约定

`init_logging(dir: String)` 接收目录路径，而不是在 Rust 侧推导 Android `filesDir` /
iOS `Library/Caches`。

- **备选**：Rust 侧用平台 API 自行解析目录。**否决** —— 需要在 Rust 里分叉两套平台代码，
  而 RN 侧本来就持有 `expo-file-system` 的目录常量，传进来最直接、也最好测。
- 副作用是宿主必须先决定目录语义。选 **cache 类目录**而非 documents：日志是短期诊断数据，
  不该进 iCloud 备份，也不该出现在用户可见的文件列表里。代价是系统存储紧张时可能被清理，
  对诊断用途可接受。

### D4：文件层用 `tracing-appender`，按天轮转

`RollingFileAppender::builder()` 提供 `rotation(DAILY)` 与 `max_log_files(n)`。

- **只能按时间轮转，不能按大小**，这是该库的硬限制。
- **备选**：自实现 `MakeWriter` 包 `file-rotate` 拿到按大小轮转。**否决** —— 复杂度陡增，
  而真正的控量手段是**少写**（见 D5），不是勤轮转。

### D5：两层用不同级别

平台原生层跟随 `EnvFilter`（开发期可放到 `debug`），**文件层固定更保守的级别**。
`swarmdrop_net` 在 P2P 场景下事件很密，文件层若也吃 `debug`，用户存储会被快速消耗。
这条同时是 D4 那个「不能按大小轮转」限制的正解。

### D6：`non_blocking` 的守卫存进核心对象

`tracing_appender::non_blocking()` 返回 `(writer, guard)`。**guard 一旦 drop，后台写线程即停止，
日志静默消失且不报任何错误**——这是该库最经典的失败模式，且没有任何反馈回路。

守卫 MUST 挂在与应用同生命周期的位置（`MobileCore` 实例或进程级 `OnceLock`），
不得留在 `init_logging()` 的栈上。这一条需要一个针对性测试兜底，否则未来重构极易复发。

### D7：RN 侧复用既有依赖

`expo-file-system`（`mobile/package.json:63`）读文件、`expo-sharing`（`:73`）拉起分享面板，
两者都已在依赖里。设置页新增入口，**零新 JS 依赖**。

## Risks / Trade-offs

- **[守卫被释放导致日志静默中断]** → 守卫存进核心对象；补一条断言其存活的测试。这是本次
  最容易在未来重构中复发、且失败完全无声的一条。
- **[日志泄露设备标识与网络地址]** → 导出前强制提示；口径与仓库 issue 模板一致。日志走系统
  分享面板意味着用户一按就发出去了，提示必须在分享之前而非之后。
- **[cache 目录被系统清理导致日志丢失]** → 接受。诊断日志是短期数据；换成 documents 会带来
  iCloud 备份与用户可见性问题，代价更大。
- **[新增 Rust 依赖增大移动端包体]** → 三个 crate 都很小，且平台层按 target 分叉、不会同时编入；
  实施时记录包体前后差值。
- **[无条件依赖会让 wasm 门禁变红]** → 依赖严格 target 门控且只落 `mobile-core`；
  实施后必须跑 `./scripts/check-wasm.sh` 确认。
- **[文件写入影响传输性能]** → 用 `non_blocking` 写入；文件层级别更保守，进一步压低写入量。

## Migration Plan

纯增量，无数据迁移、无破坏性变更。

- 改动了 uniffi 导出面，需重建桥接（`pnpm --filter react-native-swarmdrop-core build:ios`）
  并重新生成 TS 绑定，与其它 FFI 变更同流程。
- 回滚即移除 `init_logging()` 调用与设置页入口；`crates/*` 未被改动，回到「日志不产生」的现状即可，
  不留残余状态。

## Open Questions

- 文件层的默认级别与保留天数取多少？倾向 `info` + 7 天，需在真机上按实际日志量校准。
- 是否需要向用户暴露「提高日志级别」的开关（Mozilla 的 `setMaxLevel` 形状）？
  它对复现难题的用户有用，但会增加设置页面积与误用风险。倾向本次不做。
- `panic_hook.rs` 的 `eprintln!` 在 Android 上无效，是否顺手改成经订阅器输出？
  panic 信息已有 `store()` + `take_last()` 兜底未丢失，故本次只修注释，改动留作后续。
