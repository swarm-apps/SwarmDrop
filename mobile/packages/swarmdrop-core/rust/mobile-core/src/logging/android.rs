//! Android 的 logcat 输出层。
//!
//! **为什么自己写而不用现成 crate**：`tracing-logcat`（停更约 2 年）与 `tracing-android`
//! （停更约 4 年）是仅有的两个候选，都已不再维护。而这一层的全部工作只是把格式化好的
//! 字节交给 NDK 的 `__android_log_write`——liblog 在 Android 上默认链接，声明一个
//! `extern "C"` 就够了，不值得为此背一个停更依赖。
//!
//! **为什么日志不能走 stdout/stderr**：Android 把进程的 stdout/stderr 重定向到
//! `/dev/null`。老办法 `log.redirect-stdio` 只在 Dalvik（Android 4.4 及更早）有效，
//! ART（5.0 及以后）不支持——也就是在所有还活着的 Android 版本上都不管用。
//!
//! 本模块刻意把**纯逻辑**（级别映射、字节清理）与 **FFI 调用**分开：前者不带 `cfg`
//! 门控，因此 `cargo test` 在开发机（macOS）上就能覆盖，不必等到真机。

// 在非 Android 平台上本模块只为跑那几条纯逻辑测试而编译，writer 与常量自然无人使用。
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use tracing::Level;

/// logcat 的 tag。Android 对 tag 长度有限制（旧版本 23 字符），这个名字安全。
pub(super) const TAG: &str = "SwarmDrop";

/// Android `log.h` 的优先级常量。这里直接写值而不引 `ndk-sys`——只用到 5 个整数，
/// 为此多一个依赖不划算。
mod prio {
    pub const VERBOSE: i32 = 2;
    pub const DEBUG: i32 = 3;
    pub const INFO: i32 = 4;
    pub const WARN: i32 = 5;
    pub const ERROR: i32 = 6;
}

/// 单条 logcat 消息的字节上限。
///
/// logcat 的实际上限约 4096 字节且含 tag 与结尾 NUL，超出部分会被内核**静默截断**。
/// 这里主动留出余量并显式截断，好过让它在不确定的位置断开。
const MAX_PAYLOAD: usize = 3_800;

/// tracing 级别 → Android 优先级。
pub(super) fn priority_for(level: &Level) -> i32 {
    match *level {
        Level::TRACE => prio::VERBOSE,
        Level::DEBUG => prio::DEBUG,
        Level::INFO => prio::INFO,
        Level::WARN => prio::WARN,
        Level::ERROR => prio::ERROR,
    }
}

/// 把 fmt layer 产生的字节整理成能安全传给 C 的内容。
///
/// 三件事，每件都对应一个真实的失败模式：
///
/// 1. **去掉尾部换行** —— logcat 自己分条，留着会多出一行空行。
/// 2. **替换内嵌 NUL** —— `CString::new` 遇到内嵌 NUL 会返回 Err，而日志内容来自
///    用户文件名等外部输入，完全可能含 NUL。丢弃整条日志比替换成可见字符更糟。
/// 3. **截断超长内容** —— 见 [`MAX_PAYLOAD`]。截断按 UTF-8 字符边界，不切碎多字节字符。
pub(super) fn sanitize(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_end_matches(['\n', '\r']);
    let no_nul = if trimmed.contains('\0') {
        trimmed.replace('\0', "␀")
    } else {
        trimmed.to_owned()
    };

    if no_nul.len() <= MAX_PAYLOAD {
        return no_nul;
    }
    // 按字符边界截断：直接切字节会产生非法 UTF-8。
    let mut end = MAX_PAYLOAD;
    while end > 0 && !no_nul.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &no_nul[..end])
}

#[cfg(target_os = "android")]
mod ffi {
    use std::ffi::CString;

    // Android 的 liblog。NDK 默认链接，无需在 build.rs 里额外声明。
    unsafe extern "C" {
        fn __android_log_write(
            prio: std::os::raw::c_int,
            tag: *const std::os::raw::c_char,
            text: *const std::os::raw::c_char,
        ) -> std::os::raw::c_int;
    }

    /// 写一条 logcat。`text` 已由 [`super::sanitize`] 处理过，这里不会再有内嵌 NUL。
    pub(super) fn write(prio: i32, tag: &str, text: &str) {
        let (Ok(tag), Ok(text)) = (CString::new(tag), CString::new(text)) else {
            // sanitize 之后仍失败说明上游逻辑有问题；日志层不该因此让应用崩溃。
            return;
        };
        // SAFETY: 两个指针都来自存活到调用结束的 CString，且均以 NUL 结尾。
        unsafe {
            __android_log_write(prio, tag.as_ptr(), text.as_ptr());
        }
    }
}

/// 非 Android 平台的空实现，让本模块能在开发机上编译、从而被测试覆盖。
#[cfg(not(target_os = "android"))]
mod ffi {
    pub(super) fn write(_prio: i32, _tag: &str, _text: &str) {}
}

/// `tracing-subscriber` 的 writer：每条事件构造一个，`Drop` 时落到 logcat。
pub(super) struct LogcatWriter {
    prio: i32,
    buf: Vec<u8>,
}

impl LogcatWriter {
    fn new(prio: i32) -> Self {
        Self {
            prio,
            buf: Vec::new(),
        }
    }

    fn emit(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = sanitize(&self.buf);
        self.buf.clear();
        if !text.is_empty() {
            ffi::write(self.prio, TAG, &text);
        }
    }
}

impl std::io::Write for LogcatWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.emit();
        Ok(())
    }
}

impl Drop for LogcatWriter {
    fn drop(&mut self) {
        // fmt layer 不保证每条事件后都调 flush，所以 Drop 是最后一道保证。
        self.emit();
    }
}

/// 挂进 `fmt::layer().with_writer(..)` 的工厂。
pub(super) struct MakeLogcatWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for MakeLogcatWriter {
    type Writer = LogcatWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogcatWriter::new(prio::INFO)
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        LogcatWriter::new(priority_for(meta.level()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_level_to_android_priority() {
        assert_eq!(priority_for(&Level::TRACE), 2);
        assert_eq!(priority_for(&Level::DEBUG), 3);
        assert_eq!(priority_for(&Level::INFO), 4);
        assert_eq!(priority_for(&Level::WARN), 5);
        assert_eq!(priority_for(&Level::ERROR), 6);
    }

    #[test]
    fn strips_trailing_newlines() {
        assert_eq!(sanitize(b"hello\n"), "hello");
        assert_eq!(sanitize(b"hello\r\n"), "hello");
        assert_eq!(sanitize(b"a\nb\n"), "a\nb");
    }

    /// 内嵌 NUL 会让 `CString::new` 失败。日志内容含用户文件名等外部输入，
    /// 必须替换而不是丢弃整条。
    #[test]
    fn replaces_embedded_nul_instead_of_dropping_the_line() {
        let out = sanitize(b"before\0after");
        assert!(!out.contains('\0'));
        assert!(out.contains("before") && out.contains("after"));
        assert!(std::ffi::CString::new(out).is_ok());
    }

    #[test]
    fn truncates_overlong_payload_on_char_boundary() {
        let long = "★".repeat(4_000); // 每个 3 字节，远超上限
        let out = sanitize(long.as_bytes());
        assert!(out.len() <= MAX_PAYLOAD + 4, "实际 {}", out.len());
        assert!(out.ends_with('…'));
        // 关键：截断后仍是合法 UTF-8，没有把多字节字符切碎。
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }

    #[test]
    fn keeps_non_ascii_intact_when_within_limit() {
        assert_eq!(sanitize("配对成功 ✅".as_bytes()), "配对成功 ✅");
    }

    /// 非法 UTF-8 不得 panic —— 日志内容可能来自任意字节流。
    #[test]
    fn tolerates_invalid_utf8() {
        let out = sanitize(&[0xff, 0xfe, b'o', b'k']);
        assert!(out.contains("ok"));
    }
}
