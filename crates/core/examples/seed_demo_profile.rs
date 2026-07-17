//! Demo profile seeder —— 为录制 / 截图生成一个「干净、可复现、隐私安全」的 fixture profile。
//!
//! 往 `SWARMDROP_DATA_DIR`（或第一个命令行参数）指定的目录写入：
//! - `dev-identity.json`：新生成的 self keypair + 一组通用名的已配对设备（**合法假 PeerId**，
//!   由真 Ed25519 keypair 派生，才能通过 app 的 `PeerId` 反序列化校验）
//! - `device_config.json`：通用本机设备名
//!
//! 与 app 的 `file_keychain` 复用同一份 [`PairedDeviceInfo`] 结构 + serde 派生，schema 永远对齐。
//! 只含虚构设备名 / 假 PeerId，满足 `e2e/desktop/demo-postproduction-design.md` §6 隐私治理。
//!
//! 用法（配合 `SWARMDROP_DATA_DIR` 覆盖，见 `src-tauri/src/host/paths.rs`）：
//! ```bash
//! SWARMDROP_DATA_DIR=/path/to/fixture cargo run -p swarmdrop-core --example seed_demo_profile
//! # 或显式传目录
//! cargo run -p swarmdrop-core --example seed_demo_profile -- /path/to/fixture
//! ```

use std::path::PathBuf;

use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::device::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};

/// 构造一台已配对设备：真 Ed25519 keypair → 合法 PeerId，通用元数据。
fn device(
    name: &str,
    os: &str,
    platform: &str,
    arch: &str,
    paired_at: i64,
    trust_level: DeviceTrustLevel,
    trust_confirmed: bool,
) -> PairedDeviceInfo {
    let peer_id = Keypair::generate_ed25519().public().to_peer_id();
    let os_info = OsInfo {
        name: Some(name.to_string()),
        hostname: name.to_lowercase().replace(' ', "-"),
        os: os.to_string(),
        platform: platform.to_string(),
        arch: arch.to_string(),
        capabilities: Vec::new(),
    };
    let mut info = PairedDeviceInfo::new(peer_id, os_info, paired_at);
    info.apply_trust_level_defaults(trust_level);
    // apply_* 会把 trust_confirmed 置 true；待确认设备需显式改回。
    info.trust_confirmed = trust_confirmed;
    info
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SWARMDROP_DATA_DIR").ok())
        .map(PathBuf::from)
        .ok_or("需要输出目录：设置 SWARMDROP_DATA_DIR 或传第一个参数")?;
    std::fs::create_dir_all(&dir)?;

    // self 身份：新生成，protobuf 编码（与 app `DeviceIdentityBytes.keypair` 存储格式一致）。
    let self_keypair = Keypair::generate_ed25519().to_protobuf_encoding()?;

    // 固定基准时间戳，保证每次 seed 结果稳定可复现。
    let base = 1_720_000_000_i64;
    let devices = vec![
        device(
            "MacBook Air",
            "macOS 15",
            "macos",
            "aarch64",
            base,
            DeviceTrustLevel::Owned,
            true,
        ),
        device(
            "iPhone 15 Pro",
            "iOS 18",
            "ios",
            "aarch64",
            base - 3_600,
            DeviceTrustLevel::Collaborator,
            true,
        ),
        device(
            "Pixel 8",
            "Android 15",
            "android",
            "aarch64",
            base - 7_200,
            DeviceTrustLevel::Collaborator,
            true,
        ),
        device(
            "Windows 工作站",
            "Windows 11",
            "windows",
            "x86_64",
            base - 86_400,
            DeviceTrustLevel::Collaborator,
            true,
        ),
        // 待确认：trust_confirmed=false → UI 显示「协作者 · 待确认」
        device(
            "iPad Pro",
            "iPadOS 18",
            "ios",
            "aarch64",
            base - 600,
            DeviceTrustLevel::Collaborator,
            false,
        ),
        device(
            "工作室 Linux",
            "Ubuntu 24.04",
            "linux",
            "x86_64",
            base - 172_800,
            DeviceTrustLevel::Temporary,
            true,
        ),
    ];

    // 复用 app 的 dev-identity.json schema（camelCase）：{ keypair, migrationCompleted, pairedDevices }。
    let identity = serde_json::json!({
        "keypair": serde_json::to_value(&self_keypair)?,
        "migrationCompleted": true,
        "pairedDevices": serde_json::to_value(&devices)?,
    });

    let identity_path = dir.join("dev-identity.json");
    std::fs::write(&identity_path, serde_json::to_string_pretty(&identity)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))?;
    }

    let device_config = serde_json::json!({ "device_name": "SwarmDrop 演示机" });
    std::fs::write(
        dir.join("device_config.json"),
        serde_json::to_string_pretty(&device_config)?,
    )?;

    println!("✓ demo profile 已写入 → {}", dir.display());
    println!(
        "  self keypair: {} bytes · 已配对设备: {}",
        self_keypair.len(),
        devices.len()
    );
    for d in &devices {
        println!(
            "  - {:<16} [{:?}{}]",
            d.os_info.name.as_deref().unwrap_or("?"),
            d.trust_level,
            if d.trust_confirmed {
                ""
            } else {
                " · 待确认"
            },
        );
    }
    Ok(())
}
