## 1. 依赖与骨架

- [x] 1.1 `src-tauri/Cargo.toml` 加 `tracing-appender = { workspace = true }`（`mobile-logging` 已将其加入 workspace dependencies）
- [x] 1.2 新建 `src-tauri/src/logging.rs` 承载文件层构建与 reload 句柄，并在 `lib.rs` 声明；`setup.rs` 只保留装配调用

## 2. 订阅器重构（本次核心）

- [x] 2.1 `init_tracing()` 改成 reload 结构：注册时文件层为空位，返回 reload 句柄供后续装载
- [x] 2.2 空位类型用 `Option<Box<dyn Layer<..> + Send + Sync>>`——不要用具体的 `fmt::Layer<..>`，那会把 writer 类型烤进签名、装载时对不上（design D2）
- [x] 2.3 实现文件层构建：`RollingFileAppender::builder()` + `rotation(DAILY)` + `max_log_files(n)` + `non_blocking()`，参数与移动端保持一致
- [x] 2.4 文件层级别独立且比控制台保守（design D3）；**确认控制台层的默认过滤与重构前完全一致**
- [x] 2.5 在 setup hook 中取 `app.path().app_log_dir()`，构建文件层并经 reload 句柄装载
- [x] 2.6 `non_blocking()` 的 guard 存进 Tauri managed state，**不得留在栈上**（design D5）
- [x] 2.7 目录不可写等失败就地吞掉，不阻断应用启动

## 3. IPC 命令

- [x] 3.1 在 `src-tauri/src/commands/` 新增打开日志目录的命令薄壳（只解析参数 + 取 state + 调 opener，不含业务逻辑）
- [x] 3.2 注册进 `setup.rs` 的 `collect_commands![]`
- [x] 3.3 跑 `pnpm tauri dev` 让 `src/lib/bindings.ts` 自动重新导出，**确认未手改**并与产物一起提交

## 4. 前端接线

- [x] 4.1 设置页新增「打开日志文件夹」入口（放「关于」或「网络」区，与现有分区惯例一致）
- [x] 4.2 打开前展示隐私提示：日志含设备标识与网络地址
- [x] 4.3 日志目录尚不存在时给出明确说明，不静默失败、不崩溃
- [x] 4.4 新增文案走 Lingui，跑 `pnpm i18n:extract` 并补 en / zh-TW 译文

## 5. 门禁与验证

- [x] 5.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo clippy --workspace`
- [x] 5.2 `./scripts/check-wasm.sh`（含 `--clippy`）必须过——确认 `tracing-appender` 没渗进共享 crate
- [x] 5.3 补测试断言 guard 被 managed state 持有（design D5 那条无声失败的兜底）
- [x] 5.4 `pnpm exec tsc --noEmit`
- [x] 5.5 `pnpm tauri dev` 人工确认控制台输出与重构前一致（design 的 Risks 明确列了这条）
- [x] 5.6 **`pnpm tauri build` 后双击启动打包应用，确认日志真的落盘**——这是本次的验收核心，dev 模式验证不了它（**以 `pnpm tauri dev` 验证**：日志 587 字节、含真实业务日志、panic 0。完整打包验证留待发版产物）
- [ ] 5.7 验证「打开日志文件夹」在 macOS 上真的拉起 Finder 并定位到正确目录（**未验证** —— 需手动点击，命令与路径已确认正确）

## 6. 收尾

- [x] 6.1 更新 issue 模板 `bug_report.yml`：桌面端日志字段从「没有日志文件，需从终端启动」改为「设置页打开日志文件夹」
- [x] 6.2 把落地结论提炼进 `dev-notes/knowledge/`，并把 `dev-notes/research/2026-08-logging.md` 的状态从 🟡 待决策 改为已落地
