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

/// 验证点 3：连 relay，报告每个 home relay 的真实连接状态。
///
/// `home_relay_status()` 给的是 `Vec<RelayStatus>`（可能有多个 home relay），
/// 每项带 `url()` / `is_connected()` / `last_error()`。
///
/// **不只看「有没有返回」，要看 `is_connected()`** —— watcher 初始化完成不等于
/// 握手成功。`last_error()` 是这个 spike 最有价值的东西：relay 连不上时它给出
/// 具体原因，这正是 #62 区分「iroh 不行」与「relay 够不着」所需要的。
#[wasm_bindgen]
pub async fn probe_relay() -> Result<JsValue, JsError> {
    let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
        .bind()
        .await
        .map_err(to_js_err)?;

    // initialized() 语义是「等 Nullable 从空变为有值」，Vec<RelayStatus> 的空 Vec
    // 视作 null —— 所以它解包后给的是**单个** RelayStatus，不是整个列表。
    //
    // **且它返回得很早**：relay 一进列表就返回，此时握手还在进行
    // （实测 is_connected()=false、last_error()=None）。要拿终态必须 updated() 轮候，
    // 否则会把「还在连」误报成「连不上」——这个坑正是 #62 说的「分不清是 iroh 不行
    // 还是 relay 够不着」的一种变体。
    let mut watcher = endpoint.home_relay_status();
    let mut status = watcher.initialized().await;

    // 最多等 20 秒。终态 = 连上了，或拿到了具体错误。
    let deadline = 20;
    let mut waited = 0;
    while !status.is_connected() && status.last_error().is_none() && waited < deadline {
        // updated() 等下一次状态变化；配 timeout 防止 relay 静默不回导致永久挂起
        // 注意不对称：initialized() 会把 Vec 解包成单个 RelayStatus，
        // 而 updated() 返回的是完整的 Vec<RelayStatus>。
        let next = match n0_future::time::timeout(
            std::time::Duration::from_secs(1),
            watcher.updated(),
        )
        .await
        {
            Ok(Ok(v)) => v,
            // 超时或 watcher 断开：重新取当前值再判一次
            _ => watcher.get(),
        };
        if let Some(s) = next.into_iter().next() {
            status = s;
        }
        waited += 1;
    }

    let report = match (status.is_connected(), status.last_error()) {
        (true, _) => format!("✅ {} 已连通（等待 {waited}s）", status.url()),
        // last_error 是这个 spike 最有价值的输出：它区分「iroh 不行」与「relay 够不着」
        (false, Some(err)) => format!("❌ {} 连接失败: {err}", status.url()),
        (false, None) => format!("⏱️ {} 等待 {waited}s 仍未连通，且无错误上报（疑似被静默丢弃/墙）", status.url()),
    };
    tracing::info!("{report}");

    // 同上：不 close 会触发 iroh 的 Drop 告警，且不向 relay 发优雅断开。
    endpoint.close().await;

    Ok(JsValue::from_str(&report))
}

fn to_js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&err.to_string())
}
