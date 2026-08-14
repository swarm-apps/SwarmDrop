//! Tauri Builder 装配
//!
//! 把 [`crate::run`] 拆成四段：tracing 初始化 / plugin 注册 / setup hook /
//! 命令注册。每段各司其职，方便调整。
//!
//! 命令通过 [`tauri-specta`] 收集，debug build 会同时把 TypeScript bindings
//! 导出到 `src/lib/bindings.ts`，供前端 typesafe 调用。

use std::sync::Arc;

use crate::{commands, events};
use tauri::{Builder, Manager, Wry};
use tauri_specta::{Builder as SpectaBuilder, ErrorHandlingMode, collect_commands, collect_events};

/// 初始化 tracing 订阅器（默认过滤见 [`crate::logging`] 的 `DEFAULT_FILTER`，
/// 可被 `RUST_LOG` 覆盖）。
///
/// 这里刻意不复述那个值——它已经漂移过一次（补 `webrtc_p2p` / `webrtc` / `rtc` 时
/// 这条注释没跟上），而复述一份就等于再造一个会过期的事实源。
///
/// 实现搬进 [`crate::logging`]：控制台层在这里立即生效，**文件层留一个空位**——
/// 此刻还在 `tauri::Builder` 之前，`app_log_dir()` 拿不到。等 setup hook 里
/// 取到目录后再由 `install_file_layer` 装载。
///
/// 之所以不把整个初始化推迟到 setup hook：那会丢掉启动早期的日志，而身份
/// 读取与节点 bind 恰好在那之前，且它们正是最容易出问题的阶段。
pub fn init_tracing() {
    crate::logging::init();
}

/// 构造 tauri-specta [`SpectaBuilder`]：所有 IPC 命令在此集中注册。
///
/// 单独抽出便于测试中导出 TS bindings（`src-tauri/tests/specta_export.rs`），
/// 避免依赖完整的 `tauri::Builder`。
///
/// 启用 [`SpectaBuilder::dangerously_cast_bigints_to_number`]：本应用的所有
/// `u64` / `i64` / `usize` 字段（文件大小、毫秒时间戳、字节计数）都在 JS 安全
/// 整数范围（< 2^53）内，统一映射成 TS `number` 比 `bigint` 对前端更友好。
pub fn specta_builder() -> SpectaBuilder<Wry> {
    SpectaBuilder::<Wry>::new()
        .dangerously_cast_bigints_to_number()
        // Throw 模式：生成的 commands 直接 throw，前端代码维持现有 try/catch 习惯，
        // 不用改成 `if (r.status === "error") ...` 的 Result tuple 风格。
        .error_handling(ErrorHandlingMode::Throw)
        .commands(collect_commands![
            // lifecycle
            commands::start,
            commands::shutdown,
            commands::list_devices,
            commands::get_network_status,
            commands::install_update,
            // infra（引导节点意图）
            commands::validate_infra_addr,
            commands::add_infra_node,
            commands::remove_infra_node,
            commands::supported_transports,
            // inbox
            commands::list_inbox_items,
            commands::search_inbox,
            commands::get_inbox_item_detail,
            commands::get_inbox_item_by_transfer_session_id,
            commands::repair_missing_inbox_items,
            commands::open_inbox_item,
            commands::show_inbox_item_in_folder,
            commands::export_inbox_item,
            commands::archive_inbox_item,
            commands::delete_inbox_item,
            // identity
            commands::initialize_identity,
            commands::generate_keypair,
            commands::get_identity_file_path,
            commands::register_keypair,
            commands::get_device_name,
            commands::set_device_name,
            // pairing
            commands::generate_pair_invite,
            commands::revoke_pair_invite,
            commands::list_pair_invites,
            commands::revoke_pair_invite_by_id,
            commands::invite_qr_svg,
            commands::decode_pair_invite,
            commands::consume_pair_invite,
            commands::request_pairing,
            commands::respond_pairing_request,
            commands::remove_paired_device,
            commands::default_receive_policy,
            commands::update_paired_device_policy,
            // transfer
            commands::scan_sources,
            commands::prepare_send,
            commands::send_text_delivery,
            commands::retry_text_delivery,
            commands::confirm_text_delivery,
            commands::pending_text_deliveries,
            commands::list_text_outbox,
            commands::delete_text_outbox_record,
            commands::start_send,
            commands::accept_receive,
            commands::reject_receive,
            commands::cancel_send,
            commands::cancel_receive,
            commands::pause_send,
            commands::pause_receive,
            commands::get_transfer_projections,
            commands::get_transfer_source_paths,
            commands::delete_transfer_session,
            commands::clear_transfer_history,
            commands::resume_transfer,
            commands::set_receiving_paused,
            commands::is_receiving_paused,
            // 应用窗口 / 托盘
            commands::quit_app,
            // 语言（托盘 + 系统通知等原生字符串本地化）
            commands::set_locale,
            // 诊断日志
            commands::open_log_dir,
            // mcp
            commands::get_mcp_status,
            commands::start_mcp_server,
            commands::stop_mcp_server,
            // 外部文件打开（Open With → share-target）
            commands::take_pending_external_open,
        ])
        .events(collect_events![
            events::NetworkStatusChanged,
            events::DevicesChanged,
            events::PairingRequestReceived,
            events::PairedDeviceAdded,
            events::PairedDeviceRemoved,
            events::DeviceRenamed,
            events::TransferOffer,
            events::TransferProgress,
            events::PrepareProgress,
            events::TransferAccepted,
            events::TransferRejected,
            events::TransferComplete,
            events::TransferFailed,
            events::TransferPaused,
            events::TransferResumed,
            events::TransferDbError,
            events::TransferProjectionUpdate,
            events::FilePublish,
            events::ReceivingPausedChanged,
            events::ExternalFileOpen,
            events::ExternalPairInvite,
            events::TrayOpenReceiveFolder,
            events::TrayOpenSettings,
        ])
}

/// 构造完整的 [`Builder`]：plugin → tauri-specta handler → setup hook。
pub fn build_app() -> Builder<Wry> {
    let specta = specta_builder();

    // debug build 时导出 TS bindings；release build 跳过避免误覆盖。
    #[cfg(debug_assertions)]
    {
        use specta_typescript::Typescript;
        if let Err(e) = specta.export(
            Typescript::default().header("// AUTO-GENERATED by tauri-specta. DO NOT EDIT.\n"),
            "../src/lib/bindings.ts",
        ) {
            tracing::warn!("Failed to export specta TS bindings: {e}");
        }
    }

    let builder = register_plugins(Builder::default()).invoke_handler(specta.invoke_handler());
    register_setup(builder, specta)
}

/// 注册所有官方 + 第三方 plugin。
fn register_plugins(builder: Builder<Wry>) -> Builder<Wry> {
    let builder = builder
        // single-instance 必须最先注册：常驻后台 app 二次启动时唤出已有窗口而非再起进程，
        // 避免出现两个托盘图标 / 状态错乱。
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // 无文件的二次启动：唤出主窗口。带文件的「用本应用打开」由 handle_second_instance
            // 处理（Windows/Linux 从 argv 取路径，macOS 走 RunEvent::Opened、此调用 no-op）。
            crate::tray::show_main_window(app);
            crate::external_open::handle_second_instance(app, args);
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // deep-link：**只用它的 scheme 注册能力**（macOS plist / Windows 注册表 /
        // Linux .desktop），事件消费仍走 `external_open` 那份自建 handler。
        //
        // 理由（openspec: pair-deep-link design D1）：自建 handler 带着两样不能丢的东西 ——
        // `catch_unwind`（`RunEvent::Opened` 在 ObjC extern "C" 回调里触发，panic 不能跨
        // 该边界 unwind，否则直接 abort）与全局 OnceLock 缓冲（冷启动时事件可能早于
        // `app.manage(...)`，那时访问托管状态会 panic）。
        //
        // ⚠️ **待实测**：macOS 上本 plugin 也会挂 `RunEvent::Opened`，两个消费者是否互相
        // 影响未验证（需要真机跑 `pnpm tauri dev` 并点一次深链）。若发现它抢事件，退路是
        // 不装 plugin、自己写三平台注册（见 design D1 末尾）。
        .plugin(tauri_plugin_deep_link::init());

    // MCP Bridge —— 仅 debug build 注册,供 @hypothesi/tauri-mcp-server 调试
    // (读日志 / 调 IPC / 操作 WebView)。默认监听 0.0.0.0:9223;release 不注册。
    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_mcp_bridge::init());

    // WebDriver E2E —— 仅 debug build 注册,供 e2e/desktop 下的 WebdriverIO 原生模式测试用。
    // wdio-webdriver 起内嵌 WebDriver server(embedded provider,零外部进程);wdio 提供
    // browser.tauri.execute / IPC mock / 前后端日志采集。release 不注册,攻击面仅限 dev。
    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .plugin(tauri_plugin_wdio::init());

    builder
}

/// `setup` hook：updater plugin 容错注册 + 数据库初始化 + MCP state 装配 +
/// tauri-specta event mount。
fn register_setup(builder: Builder<Wry>, specta: SpectaBuilder<Wry>) -> Builder<Wry> {
    builder.setup(move |app| {
        // 日志文件层：`init_tracing()` 只装了控制台层，文件层留了个空位——那时还在
        // `tauri::Builder` 之前，`app_log_dir()` 拿不到。这里是能取到目录的最早时机，
        // 所以放在 setup hook 的第一步，让后续所有初始化的日志都落盘。
        //
        // ⚠️ guard 必须 `manage` 住：它一旦被 drop，`non_blocking` 的后台写线程就停止，
        // 日志静默消失且**不产生任何错误**。这里不 manage 的话，它会在本闭包结束时 drop。
        //
        // 取不到目录或目录不可写时静默跳过——日志是诊断设施，不该让应用起不来。
        if let Ok(log_dir) = app.path().app_log_dir()
            && let Some(guard) = crate::logging::install_file_layer(&log_dir)
        {
            app.manage(guard);
        }

        // tauri-specta events —— 当前未声明事件，为将来扩展预留 mount 钩子。
        specta.mount_events(app);

        // 原生字符串（托盘 / 系统通知）locale：前端 preferences-store 为权威源，此处读
        // 持久化值应用到 rust-i18n，保证托盘首帧即正确语言（必须在 build_tray 之前）。
        crate::i18n::init_locale_from_store(app.handle());

        // 系统托盘：常驻图标 + 菜单。句柄存入 state 长存（被 drop 图标会消失）。
        crate::tray::build_tray(app.handle())?;

        // 平台标题栏调整:macOS 由 tauri.conf.json 的 trafficLightPosition 控制红绿灯位置;
        // Windows / Linux 需要运行时关装饰,改由前端自画最小化/最大化/关闭三按钮。
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
            }
        }

        // updater plugin —— 移动端不支持时静默跳过
        if let Err(e) = app
            .handle()
            .plugin(tauri_plugin_updater::Builder::new().build())
        {
            tracing::warn!("Failed to initialize updater plugin: {e}");
        }

        // 深链 scheme 注册：**只有 dev 需要**（打包后由 installer / plist 完成）。
        // 没有它，`pnpm tauri dev` 下点 swarmdrop:// 链接系统找不到处理程序，深链根本测不了。
        // macOS 无需运行时注册（走 bundle plist），故只对 Windows / Linux 调。
        #[cfg(all(debug_assertions, not(target_os = "macos")))]
        {
            use tauri_plugin_deep_link::DeepLinkExt as _;
            if let Err(e) = app.deep_link().register_all() {
                tracing::warn!("注册 swarmdrop:// scheme 失败（dev 下深链将不可用）: {e}");
            }
        }

        // 事件总线提前注入：启动清理也会经 Coordinator 发布 TransferProjection。
        let event_bus = crate::host::event_bus::TauriEventBus::new(app.handle().clone());
        app.manage(event_bus.clone());

        // 数据库（SeaORM + SQLite）—— 同步执行 + 启动清理过期会话
        let handle = app.handle().clone();
        let db = tauri::async_runtime::block_on(crate::database::init_database(&handle))?;

        // 传输域持久化端口的**唯一**构造点（openspec: inbox-store-port-completion design D5）。
        //
        // 注入 `TransferManager` 的（`commands::start` 的工厂闭包从 state 里取的就是这一份）
        // 与宿主自持的（收件箱命令、MCP 工具、启动清理）**必须是同一个 `Arc`**，
        // 不是两个包装同一条连接的实例——后者等于凭空多出一个事实源，
        // 缓存、事务与将来的进程内状态都会在两份实例之间悄悄分叉。
        //
        // 托管端口而非节点：收件箱按定义是与网络无关的内容账本，
        // 「没联网也能翻已经收到的东西」不该退化成「先启动节点」。
        let transfer_store: crate::database::TransferStoreState = Arc::new(
            swarmdrop_storage_sql::SqlSessionStore::new(Arc::new(db.clone())),
        );

        let cleanup_event_bus: Arc<dyn swarmdrop_core::host::EventBus> = Arc::new(event_bus);
        tauri::async_runtime::block_on(crate::database::cleanup_stale_sessions(
            &transfer_store,
            cleanup_event_bus,
        ))?;
        app.manage(transfer_store);
        app.manage(db);

        // 文件访问端口：**组装点建一次**，`start()` 注入给 TransferManager 的与收件箱命令
        // 自持的是同一个 `Arc`（与上面 transfer_store 同一条纪律）。
        //
        // 建在这里而不是 `start()` 里，是因为收件箱命令**刻意不依赖节点启动**——它是与网络
        // 无关的内容账本。`TauriFileAccess::new` 只需要 AppHandle，setup 阶段就已具备。
        let file_access: Arc<dyn swarmdrop_core::host::FileAccess> = Arc::new(
            crate::host::file_source::TauriFileAccess::new(app.handle().clone()),
        );
        app.manage(file_access);

        // MCP server 状态容器
        app.manage(crate::mcp::server::McpServerState::default());

        // 网络运行时状态容器。启动节点前为 None，避免依赖 NetManagerState 的命令
        // 在节点未启动时触发 Tauri 的 "state not managed" 注入错误。
        app.manage(tokio::sync::Mutex::new(None::<crate::network::NetManager>));

        // 外部「用 SwarmDrop 打开」：后台注册处理器 + 冷启动参数解析。缓冲用模块内全局
        // OnceLock（不依赖此处时序：macOS 冷启动 Opened 可能早于本 setup）。
        // 平台差异（macOS 走 RunEvent::Opened、其余走 argv）封装在 external_open 内部。
        crate::external_open::register_open_with();
        crate::external_open::handle_launch_args(app.handle());

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    /// 让 `cargo test` 即可再生 TS bindings（否则只有 debug 运行 app 才导出）
    #[test]
    fn export_ts_bindings() {
        use specta_typescript::Typescript;
        super::specta_builder()
            .export(
                Typescript::default().header("// AUTO-GENERATED by tauri-specta. DO NOT EDIT.\n"),
                concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/bindings.ts"),
            )
            .expect("export TS bindings");
    }
}
