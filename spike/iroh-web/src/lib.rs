//! #60 spike：验证 iroh 能否在真实浏览器里跑。
//!
//! 只回答 go/no-go 的第一关，**刻意不做 transfer / OPFS / actor**：
//!
//! 1. iroh 能否编译到 `wasm32-unknown-unknown`
//! 2. 浏览器里能否建出 `Endpoint`（拿到 EndpointId）
//! 3. 能否连上 relay
//!
//! 第 3 点是 Web 端的生死线，与原生端根本不同：iroh 官方明确
//! *"All connections from browsers to somewhere else need to flow via a relay server"*
//! —— 浏览器发不了 UDP、**没有直连兜底**，relay 不通 = Web 端完全不可用。
//!
//! API 用法照 iroh 官方 browser-echo 示例（n0-computer/iroh-examples）。
//!
//! **relay 选择见 #62**：官方建议起步用 `presets::N0`，但那对中国大陆未必成立。
//! 任何在国内跑、又默认走 N0 的实验，失败时分不清是「iroh 不行」还是「relay 够不着」。
//! 故这里把两者拆成独立入口：`create_endpoint_no_relay` 单验编译与构建，
//! `probe_relay` 单验连通性。

use iroh::{Endpoint, Watcher};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();

    tracing_subscriber::fmt()
        // 浏览器里不 without_time() 会 runtime error（wasm 无 std 时钟）
        .without_time()
        .with_ansi(false)
        .with_writer(tracing_subscriber_wasm::MakeConsoleWriter::default())
        .init();

    tracing::info!("iroh-web spike 已加载");
}

/// 验证点 1+2：不碰 relay，只看能否在浏览器里把 Endpoint 建出来。
///
/// 与 relay 分开，是为了让失败原因唯一：这个通了就证明「iroh 编进了 wasm 且能跑」，
/// 挂了则与网络无关。
///
/// **用 `Minimal` 而非 `Empty`**：`Empty` 字面意思是什么都不设（`apply` 直接返回
/// 原 builder），连**必需**的 `crypto_provider` 都不设，`bind()` 必然报
/// `InvalidCryptoProvider`。`Minimal` 只设必需项，并按 iroh 启用的 feature
/// （我们是 `tls-ring`）自动选 provider —— 正是"不碰 relay 但能跑"想要的。
///
/// 注意这个 provider 走的是 **builder 字段**，不是 rustls 的进程默认，
/// 所以 `rustls::...::install_default()` 对它无效。
#[wasm_bindgen]
pub async fn create_endpoint_no_relay() -> Result<String, JsError> {
    let endpoint = Endpoint::builder(iroh::endpoint::presets::Minimal)
        .bind()
        .await
        .map_err(to_js_err)?;

    let id = endpoint.id().to_string();
    tracing::info!(%id, "Endpoint 已建立（无 relay）");

    // 必须显式 close：iroh 在 `impl Drop for EndpointInner` 里直接 tracing::error!
    // 报 "Endpoint dropped without calling `Endpoint::close`. Aborting ungracefully."
    // —— 这是它故意的提醒，不是噪音。浏览器里尤其要紧：不 close 就不会向 relay
    // 发优雅断开，对端要等超时才知道我们走了。
    endpoint.close().await;

    Ok(id)
}

/// 验证点 3：连 relay 并等到真正连通，报告每个 home relay 的状态。
///
/// **用 `Endpoint::online()` 等连通，不要自己轮 `home_relay_status()`。**
/// 这里原先手写过一版轮询，是走弯路 —— iroh 已经把这件事封装好了
/// （`endpoint.rs:1355`），且它的实现恰好演示了两个必须避开的坑：
///
/// 1. **不要用 `initialized()`**。它等的是「Nullable 从空变为有值」，而
///    `Vec<T>` 的 `Nullable` 实现是 `self.pop()`（n0-watcher `lib.rs:121`）——
///    既**只取最后一个、丢弃其余**，又只保证「某个 relay 的 URL 已知」而非握手成功
///    （`endpoint.rs:1374` 原文：empty ... before the endpoint has selected a home relay）。
///    实测正是如此：它 1.6s 就返回，此时 `is_connected()=false` 且 `last_error()=None`。
/// 2. `online()` 内部用 `get()` 起步、`updated()` 拿完整 `Vec`、再 `.any(is_connected)`
///    —— `initialized()` 与 `updated()` 的返回类型是不对称的（前者解包成单个 T，
///    后者给完整 Value）。
///
/// `last_error()` 仍是本 spike 最有价值的输出：relay 连不上时它给出具体原因，
/// 正是 #62 区分「iroh 不行」与「relay 够不着」所需要的。
#[wasm_bindgen]
pub async fn probe_relay() -> Result<JsValue, JsError> {
    let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
        .bind()
        .await
        .map_err(to_js_err)?;

    // bind() 返回 ≠ 能用：此时几乎肯定还没选好 relay（endpoint.rs:1203 官方注释）。
    // online() 等到至少一个 relay 真正连上。配 timeout 兜底：relay 被静默丢弃时
    // online() 会永久挂起（它内部对 watcher 断开的处理是 pending 而非报错）。
    let timed_out = n0_future::time::timeout(std::time::Duration::from_secs(20), endpoint.online())
        .await
        .is_err();

    // 无论是否超时都取一次状态，这样能拿到 last_error。
    let statuses = endpoint.home_relay_status().get();

    let report = if statuses.is_empty() {
        "❌ 未选出任何 home relay（relay 完全不可达）".to_string()
    } else {
        statuses
            .iter()
            .map(|s| match (s.is_connected(), s.last_error()) {
                (true, _) => format!("✅ {} 已连通", s.url()),
                (false, Some(err)) => format!("❌ {} 连接失败: {err}", s.url()),
                (false, None) if timed_out => {
                    format!("⏱️ {} 20s 未连通且无错误上报（疑似被静默丢弃）", s.url())
                }
                (false, None) => format!("⏳ {} 仍在连接中", s.url()),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    tracing::info!("{report}");

    // 必须 close：iroh 在 impl Drop 里直接 tracing::error! + abort()（socket.rs:220）。
    endpoint.close().await;

    Ok(JsValue::from_str(&report))
}

fn to_js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&err.to_string())
}
