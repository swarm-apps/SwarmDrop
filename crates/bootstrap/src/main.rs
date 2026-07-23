use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use swarm_bootstrap::{BootstrapConfig, util};
use swarmdrop_net::RelayServerConfig;

const DEFAULT_MAX_RESERVATIONS: usize = 128;
const DEFAULT_MAX_RESERVATIONS_PER_PEER: usize = 4;
const DEFAULT_RESERVATION_DURATION_SECS: u64 = 3600;
const DEFAULT_MAX_CIRCUITS: usize = 16;
const DEFAULT_MAX_CIRCUITS_PER_PEER: usize = 4;
const DEFAULT_MAX_CIRCUIT_DURATION_SECS: u64 = 12 * 3600;

/// SwarmDrop 的 DHT 引导 + Relay + 浏览器入口节点。
#[derive(Parser, Debug)]
#[command(name = "swarm-bootstrap", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 启动引导 + relay 服务。
    Run {
        /// TCP 监听端口。
        #[arg(long, env = "SWARM_BOOTSTRAP_TCP_PORT", default_value_t = 4001)]
        tcp_port: u16,
        /// QUIC 监听端口。
        #[arg(long, env = "SWARM_BOOTSTRAP_QUIC_PORT", default_value_t = 4001)]
        quic_port: u16,
        /// 给浏览器使用的 WebSocket 监听端口。
        #[arg(long, env = "SWARM_BOOTSTRAP_WEBSOCKET_PORT", default_value_t = 4002)]
        websocket_port: u16,
        /// 给浏览器使用的 WebRTC Direct UDP 监听端口。
        #[arg(long, env = "SWARM_BOOTSTRAP_WEBRTC_PORT", default_value_t = 4003)]
        webrtc_port: u16,
        /// 身份密钥文件；默认在二进制所在目录创建 identity.key。
        #[arg(long, env = "SWARM_BOOTSTRAP_KEY_FILE")]
        key_file: Option<PathBuf>,
        /// WebRTC Direct 证书文件；默认与身份密钥同目录的 webrtc.pem。
        #[arg(long, env = "SWARM_BOOTSTRAP_WEBRTC_CERT_FILE")]
        webrtc_cert_file: Option<PathBuf>,
        /// 监听 IP 地址。
        #[arg(long, env = "SWARM_BOOTSTRAP_LISTEN_IP", default_value = "0.0.0.0")]
        listen_ip: IpAddr,
        /// 节点实际对外公布的公网 IP；relay reservation 必填。
        #[arg(long, env = "SWARM_BOOTSTRAP_EXTERNAL_IP")]
        external_ip: IpAddr,
        /// 空闲连接超时（秒）。
        #[arg(long, env = "SWARM_BOOTSTRAP_IDLE_TIMEOUT_SECS", default_value_t = 120)]
        idle_timeout_secs: u64,
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_RESERVATIONS", default_value_t = DEFAULT_MAX_RESERVATIONS)]
        max_reservations: usize,
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_RESERVATIONS_PER_PEER", default_value_t = DEFAULT_MAX_RESERVATIONS_PER_PEER)]
        max_reservations_per_peer: usize,
        #[arg(long, env = "SWARM_BOOTSTRAP_RESERVATION_DURATION_SECS", default_value_t = DEFAULT_RESERVATION_DURATION_SECS)]
        reservation_duration_secs: u64,
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_CIRCUITS", default_value_t = DEFAULT_MAX_CIRCUITS)]
        max_circuits: usize,
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_CIRCUITS_PER_PEER", default_value_t = DEFAULT_MAX_CIRCUITS_PER_PEER)]
        max_circuits_per_peer: usize,
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_CIRCUIT_DURATION_SECS", default_value_t = DEFAULT_MAX_CIRCUIT_DURATION_SECS)]
        max_circuit_duration_secs: u64,
        /// 单 circuit 最大转发字节；0 表示不限。
        #[arg(long, env = "SWARM_BOOTSTRAP_MAX_CIRCUIT_BYTES", default_value_t = 0)]
        max_circuit_bytes: u64,
    },
    /// 打印当前节点 PeerId。
    PeerId {
        #[arg(long, env = "SWARM_BOOTSTRAP_KEY_FILE")]
        key_file: Option<PathBuf>,
    },
}

fn resolve_key_file(key_file: Option<PathBuf>) -> PathBuf {
    key_file.unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|dir| dir.join("identity.key")))
            .unwrap_or_else(|| PathBuf::from("identity.key"))
    })
}

fn resolve_webrtc_cert_file(cert_file: Option<PathBuf>, key_file: &Path) -> PathBuf {
    cert_file.unwrap_or_else(|| key_file.with_file_name("webrtc.pem"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::PeerId { key_file } => {
            let secret = util::identity::load_or_generate_secret_key(&resolve_key_file(key_file))?;
            println!("{}", secret.node_id());
        }
        Command::Run {
            tcp_port,
            quic_port,
            websocket_port,
            webrtc_port,
            key_file,
            webrtc_cert_file,
            listen_ip,
            external_ip,
            idle_timeout_secs,
            max_reservations,
            max_reservations_per_peer,
            reservation_duration_secs,
            max_circuits,
            max_circuits_per_peer,
            max_circuit_duration_secs,
            max_circuit_bytes,
        } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            if max_circuit_duration_secs > u32::MAX as u64 {
                bail!("max circuit duration must not exceed {} seconds", u32::MAX);
            }

            let key_file = resolve_key_file(key_file);
            let cert_file = resolve_webrtc_cert_file(webrtc_cert_file, &key_file);
            let secret_key = util::identity::load_or_generate_secret_key(&key_file)?;
            let webrtc_certificate_pem =
                util::identity::load_or_generate_webrtc_certificate(&cert_file)?;
            let relay_limits = RelayServerConfig {
                max_reservations,
                max_reservations_per_peer,
                reservation_duration: Duration::from_secs(reservation_duration_secs),
                max_circuits,
                max_circuits_per_peer,
                max_circuit_duration: Duration::from_secs(max_circuit_duration_secs),
                max_circuit_bytes: if max_circuit_bytes == 0 {
                    u64::MAX
                } else {
                    max_circuit_bytes
                },
                announce_private_addrs: false,
            };
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(swarm_bootstrap::run(BootstrapConfig {
                    secret_key,
                    webrtc_certificate_pem,
                    listen_ip,
                    external_ip,
                    tcp_port,
                    quic_port,
                    websocket_port,
                    webrtc_port,
                    idle_timeout: Duration::from_secs(idle_timeout_secs),
                    relay_limits,
                }))?;
        }
    }
    Ok(())
}
