# mobile-diagnostic-log Specification

## Purpose
TBD - created by archiving change mobile-logging. Update Purpose after archive.
## Requirements
### Requirement: 移动端日志订阅器初始化

移动端 SHALL 在应用启动时初始化 `tracing` 订阅器，使 `crates/*` 中既有的 `tracing` 宏产生
实际输出。初始化入口 MUST 经 uniffi 暴露给宿主，由 RN 侧在启动流程中调用。

初始化 MUST 是幂等的：重复调用不得 panic，也不得挂上第二份订阅器造成日志重复。

#### Scenario: 应用启动后日志开始产生

- **WHEN** RN 侧在启动流程中调用 `init_logging()`
- **THEN** 订阅器完成注册
- **THEN** 此后 `crates/*` 中的 `tracing::info!` / `debug!` 产生实际输出，不再是空操作

#### Scenario: 重复调用保持幂等

- **WHEN** `init_logging()` 在同一进程内被调用第二次
- **THEN** 不 panic，不重复注册订阅器
- **THEN** 日志不出现重复条目

#### Scenario: 初始化失败不影响应用启动

- **WHEN** 日志目录不可写或订阅器注册失败
- **THEN** 应用照常启动并可正常收发文件
- **THEN** 失败被吞掉而非向上传播成崩溃

### Requirement: 平台原生日志输出

移动端 SHALL 通过各平台的原生日志设施输出日志，供开发者实时查看。
Android MUST 输出到 logcat，iOS MUST 输出到 os_log。

该输出 MUST NOT 依赖 stdout / stderr：Android 上进程的 stdout/stderr 被重定向到
`/dev/null`，`log.redirect-stdio` 仅在 Dalvik（Android 4.4 及更早）有效，
ART（5.0 及以后）不支持。

#### Scenario: Android 上日志进入 logcat

- **WHEN** 应用在 Android 设备上运行并产生日志
- **THEN** 日志经原生日志 API 写入 logcat
- **THEN** `adb logcat` 能看到这些条目

#### Scenario: iOS 上日志进入 os_log

- **WHEN** 应用在 iOS 设备上运行并产生日志
- **THEN** 日志写入 os_log
- **THEN** Console.app 与 Xcode 控制台能看到这些条目

### Requirement: 日志落盘与轮转

移动端 SHALL 将日志写入应用沙箱内的文件，作为终端用户唯一可取得的日志途径
（移动端用户无法从终端启动应用）。

文件日志 SHALL 按天轮转，并限制保留文件数量，避免无限增长占用用户存储。
文件层的级别 SHALL 独立于平台原生层并更保守，以控制写入量。

写入 MUST 是非阻塞的，且负责刷写的后台任务 MUST 在应用生命周期内保持存活——
若其提前终止，日志会静默丢失且不产生任何错误。

#### Scenario: 日志写入沙箱文件

- **WHEN** 订阅器已初始化且产生了一条 `info` 级别日志
- **THEN** 该条目被写入应用沙箱内的当前日志文件

#### Scenario: 跨天轮转

- **WHEN** 应用持续运行跨过自然日边界
- **THEN** 新日志写入新的日志文件，旧文件保留

#### Scenario: 超出保留数量时清理最旧文件

- **WHEN** 日志文件数量超过配置的保留上限
- **THEN** 最旧的文件被自动删除
- **THEN** 保留的文件数不超过上限

#### Scenario: 后台刷写任务保持存活

- **WHEN** 应用完成初始化并进入正常运行
- **THEN** 负责刷写的后台任务及其守卫在应用生命周期内持续存活
- **THEN** 日志不会因守卫被释放而静默中断

### Requirement: 用户可导出日志

移动端 SHALL 在设置界面提供导出入口，让用户把日志文件交给开发者。
导出 SHALL 经系统分享面板完成。

导出入口 MUST 在用户发起分享前提示日志内容包含设备标识与网络地址，
口径与仓库 issue 模板中的隐私提示一致。

#### Scenario: 用户导出日志

- **WHEN** 用户在设置页点击「导出日志」
- **THEN** 界面先展示日志含设备标识与网络地址的提示
- **THEN** 用户确认后拉起系统分享面板，附带当前日志文件

#### Scenario: 尚无日志文件

- **WHEN** 用户点击导出但日志文件尚未生成
- **THEN** 给出明确的空状态说明
- **THEN** 不拉起分享面板，也不报错崩溃

### Requirement: 日志路径可被宿主获取

移动端 SHALL 经 uniffi 向宿主暴露当前日志文件路径，使 RN 侧无需知道 Rust 侧的目录约定
即可读取与分享该文件。

#### Scenario: RN 侧取得日志路径

- **WHEN** RN 侧调用 `log_file_path()`
- **THEN** 返回当前日志文件在应用沙箱内的绝对路径
- **THEN** 该路径可直接交给文件读取与分享 API 使用

#### Scenario: 日志未初始化时取路径

- **WHEN** 在 `init_logging()` 之前调用 `log_file_path()`
- **THEN** 返回空值而非无效路径
- **THEN** 调用方据此展示空状态

