fn main() {
    // 集中定义 cfg alias，全 crate 用 #[cfg(wasm_browser)] / #[cfg(not(wasm_browser))]，
    // 不散落长 target 字符串（学 iroh build.rs）。
    cfg_aliases::cfg_aliases! {
        wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
    }
}
