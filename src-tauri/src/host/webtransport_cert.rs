//! WebTransport 证书对的桌面落点（`app_local_data_dir/webtransport-cert.pem`）。
//!
//! **只管路径**，读写实现是共享的
//! [`WebTransportFileCertificateStore`](swarmdrop_net::WebTransportFileCertificateStore) ——
//! 与设备名（`JsonFileDeviceConfig` + 各端给路径）同一体例。那份实现里有三条容易写错
//! 且没有反馈回路的不变量（原子写 / `0600` / 读失败不降级），不该在桌面和移动端各存一份。
//!
//! # 为什么不挂在 `KeychainProvider` 上
//!
//! 那个端口的三组方法都是「读一次就完」的形态：身份密钥与 webrtc-direct 证书都**永不
//! 改变**，宿主启动时交出去，之后再不过问。WebTransport 的证书受 spec 约束必须 ≤14 天
//! 有效期，内核会自行轮换并**回写** —— 它需要的是一个长期持有的可写端口。
//!
//! 顺带省掉一笔与本功能无关的代价：那个 trait 经 uniffi 跨 FFI，加方法要动移动端 4 个
//! 入库的生成文件。
//!
//! # 安全形态
//!
//! 文件里**有私钥**，与 `identity.json` 同目录、同一条边界：防的是「其他用户」，不防
//! 「同用户下的其他进程」（理由见 `CLAUDE.md` 的身份存储那节）。

use std::path::PathBuf;

use tauri::AppHandle;

/// 证书文件名。**改它等于让所有存量桌面端重新生成证书**（旧文件读不到 → 当作首启），
/// 于是每个对端记下的 WebTransport 地址全部失效一次。
const CERT_FILE: &str = "webtransport-cert.pem";

/// 桌面端的证书落点。
pub fn cert_path(app: &AppHandle) -> crate::AppResult<PathBuf> {
    Ok(super::paths::app_local_data_dir(app)?.join(CERT_FILE))
}

// 这里刻意没有测试：`cert_path` 只是 `app_local_data_dir(app)?.join(CERT_FILE)`，而
// `AppHandle` 在单测里构造不出来。此前那条「测试」断言的是 `Path::join` 的行为（即标准库），
// 把 `cert_path` 整个删掉它照样绿 —— 假测试比没有测试更糟，它让人以为这里有人看着。
// 读写行为的用例住在 `crates/net` 的 `cert_store`（那份是两端共用的真实现）。
