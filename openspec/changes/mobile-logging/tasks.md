## 1. Rust 侧依赖与骨架

- [x] 1.1 在 `mobile-core/Cargo.toml` 加 `tracing-subscriber`（含 `env-filter`）与 `tracing-appender`
- [x] 1.2 加 iOS 平台条件依赖：`[target.'cfg(target_os = "ios")'.dependencies]` 下 `tracing-oslog`；确认**不落入任何共享 crate**（Android 侧自实现，无需依赖，见 design D2）
- [x] 1.3 新建 `mobile-core/src/logging.rs` 并在 `lib.rs` 声明模块

## 2. 订阅器实现

- [x] 2.1 用 `tracing_subscriber::registry()` 搭三层结构：平台原生层 + 文件层 + `EnvFilter`
- [x] 2.2 平台原生层按 `cfg(target_os)` 分叉：iOS 挂 `tracing-oslog` layer；两端之外的 target 退化为无平台层（保证 `cargo check --workspace` 在 macOS 上仍过）
- [x] 2.2a Android 自实现 logcat writer：`extern "C"` 声明 `__android_log_write`，实现 `MakeWriter`，映射 tracing 级别到 Android 优先级，处理 tag 与 NUL 结尾约束（design D2）
- [x] 2.2b 为 2.2a 补测试：级别映射正确、含内嵌 NUL 或超长内容不 panic
- [x] 2.3 文件层用 `tracing_appender::rolling::Builder`：`rotation(DAILY)` + `max_log_files(n)` + 固定文件名前缀
- [x] 2.4 文件层套 `non_blocking()`，并把返回的 guard 存进与应用同生命周期的位置（`MobileCore` 实例或进程级 `OnceLock`），**不得留在 `init_logging()` 栈上**
- [x] 2.5 文件层级别独立且比平台层保守（见 design D5），避免 `swarmdrop_net` 的高频事件撑爆用户存储
- [x] 2.6 用 `OnceLock`/`Once` 做幂等保护：重复调用不 panic、不重复注册、日志不重复
- [x] 2.7 初始化失败（目录不可写等）就地吞掉并返回，不向上传播成崩溃

## 3. uniffi 导出面

- [x] 3.1 导出 `init_logging(dir: String)`，目录由宿主传入（design D3）
- [x] 3.2 导出 `log_file_path() -> Option<String>`；未初始化时返回空值而非无效路径
- [x] 3.3 重建桥接 `pnpm --filter react-native-swarmdrop-core build:ios` 并重新生成 TS 绑定，与产物一起提交

## 4. RN 侧接线

- [x] 4.1 在启动流程中调用 `initLogging()`，目录取自 `expo-file-system` 的 cache 类目录（非 documents，见 design D3）
- [x] 4.2 设置页新增「导出日志」入口
- [x] 4.3 导出前展示隐私提示（日志含设备标识与网络地址），**提示必须在拉起分享面板之前**
- [x] 4.4 用 `expo-sharing` 拉起系统分享面板，附带 `logFilePath()` 返回的文件
- [x] 4.5 日志文件尚未生成时展示空状态，不拉起面板、不报错
- [x] 4.6 新增文案走 Lingui，跑 `pnpm i18n:extract` 并补 en 译文

## 5. 修正既有注释

- [x] 5.1 修 `mobile-core/src/panic_hook.rs:45` 的注释——「Android logcat / iOS oslog 都会捕获 stderr」对 Android 不成立；注明 panic 信息由 `store()` + `take_last()` 兜底，`eprintln!` 那行在 Android 上不产生输出

## 6. 门禁与验证

- [x] 6.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo clippy --workspace`
- [x] 6.2 **`./scripts/check-wasm.sh`（含 `--clippy`）必须过** —— 验证新依赖没有渗进共享 crate
- [x] 6.3 补一条测试断言 guard 在初始化后仍存活（design D6 那条无声失败的兜底）
- [x] 6.4 补测试覆盖幂等：连续两次 `init_logging()` 不 panic、不重复注册
- [x] 6.5 在 `mobile/` 下跑 `pnpm typecheck`
- [x] 6.6 Android 真机/模拟器验证：`adb logcat` 能看到日志条目
- [x] 6.7 iOS 真机/模拟器验证：Console.app 能看到日志条目
- [ ] 6.8 真机验证落盘与导出：产生日志 → 设置页导出 → 分享面板出现且文件内容正确（落盘已在 iOS/Android 双端验证；**分享面板那一步需手动点**，Expo 坐标点击不生效、iOS UI 自动化要 WebDriverAgent）
- [ ] 6.9 记录接入前后的移动端包体差值，确认新依赖代价可接受（⚠️ **基线已不可得**——改动已在树上，debug 产物含调试信息无参考价值。建议下次发版用 release 产物对比）

## 7. 收尾

- [x] 7.1 把落地结论从 `dev-notes/research/2026-08-logging.md` 提炼进 `dev-notes/knowledge/`（research 记「为什么这么选」，knowledge 记「现行架构的事实」），并把该篇状态从 🟡 待决策 改为已落地
- [x] 7.2 更新 issue 模板 `bug_report.yml` 的日志字段——移动端从「拿不到」改为「设置页可导出」
