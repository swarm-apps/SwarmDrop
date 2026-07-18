fn main() {
    // 与内核同款 cfg alias：`#[cfg(wasm_browser)]` 门控整 crate，native 下空壳。
    cfg_aliases::cfg_aliases! {
        wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
    }
}
