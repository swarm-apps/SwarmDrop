//! 全局 panic hook —— 把 panic 详情(location + message + backtrace)缓存起来,
//! RN 端在 catch 到 uniffi "Rust panic" 时可调 `last_panic_message()` 拉详情。
//!
//! 为什么要做这一层:
//! - ubrn 把 panic 序列化成 RustBuffer 失败时,RN 拿到的 message 只有固定字符串
//!   `"Rust panic"`,无法定位。
//! - Android/iOS 物理机上开发者没法方便地看到 Rust 端 println/eprintln 输出。
//! - 通过 FFI getter 把 panic 详情主动暴露给 JS 端,可以 console.error 输出。

use std::backtrace::{Backtrace, BacktraceStatus};
use std::sync::{Mutex, OnceLock};

static LAST_PANIC: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

fn store() -> &'static Mutex<Option<String>> {
    LAST_PANIC.get_or_init(|| Mutex::new(None))
}

pub(crate) fn install() {
    HOOK_INSTALLED.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());

            // force_capture: 移动平台默认未启用 RUST_BACKTRACE,显式强制捕获;
            // release 包没 debug info 也至少能拿到地址供后处理。
            let backtrace = Backtrace::force_capture();
            let bt_text = match backtrace.status() {
                BacktraceStatus::Captured => format!("\n{backtrace}"),
                _ => String::new(),
            };

            let formatted = format!("panic at {location}: {payload}{bt_text}");

            // 写日志(Android logcat / iOS oslog 都会捕获 stderr)
            eprintln!("[mobile-core panic] {formatted}");

            if let Ok(mut guard) = store().lock() {
                *guard = Some(formatted);
            }
            prev(info);
        }));
    });
}

pub(crate) fn take_last() -> Option<String> {
    store().lock().ok().and_then(|mut g| g.take())
}
