# swarm-bootstrap

SwarmDrop 的公网 DHT 引导与 Circuit Relay 节点。它复用本仓重构后的
`swarmdrop-net::Endpoint`，与桌面、移动端和 WASM 端使用同一套 libp2p 版本及协议：

- Kademlia DHT Server（`/swarmdrop/2.0.0` identify）；
- Circuit Relay 与 AutoNAT v2 Server；
- TCP、QUIC；
- 浏览器可拨的 WebSocket 与 WebRTC Direct。

身份密钥 (`identity.key`) 和 WebRTC 证书 (`webrtc.pem`) 都会持久化。后者必须保留，
因为其 `certhash` 已包含在 WebRTC Direct multiaddr 中；丢失会让之前分享的地址失效。

## 构建与运行

```bash
cargo build --release -p swarm-bootstrap

./target/release/swarm-bootstrap run \
  --external-ip <VPS_PUBLIC_IP>
```

`--external-ip` 是必填项：它既是 relay reservation 返回给客户端的公网地址，也用于
从实际监听到的 WebRTC Direct `certhash` 地址构造可公告地址。仅监听 `0.0.0.0` 而未配置
公网地址会导致客户端因 `NoAddressesInReservation` 拒绝 reservation。

默认端口如下；TCP/QUIC 可以共用 `4001`，WebSocket 与 WebRTC Direct 需使用独立端口。

| 用途 | 端口 | 协议 |
| --- | --- | --- |
| 原生 TCP | 4001 | TCP |
| 原生 QUIC | 4001 | UDP |
| 浏览器 WebSocket | 4002 | TCP |
| 浏览器 WebRTC Direct | 4003 | UDP |

防火墙需放行以上四个端口（按相应 TCP/UDP 协议）。

启动后获取节点身份：

```bash
./target/release/swarm-bootstrap peer-id
```

将输出的 PeerId 与下列地址一起配置给客户端；WebRTC 地址在日志中可见，也可从
节点的 identify 地址中取得：

```text
/ip4/<VPS_PUBLIC_IP>/tcp/4001/p2p/<PEER_ID>
/ip4/<VPS_PUBLIC_IP>/udp/4001/quic-v1/p2p/<PEER_ID>
/ip4/<VPS_PUBLIC_IP>/tcp/4002/ws/p2p/<PEER_ID>
/ip4/<VPS_PUBLIC_IP>/udp/4003/webrtc-direct/certhash/<CERT_HASH>/p2p/<PEER_ID>
```

日志中的 WebRTC Direct 公告地址不含节点身份；将其配置给客户端时，必须在 `certhash` 后追加
`/p2p/<PEER_ID>`，构成上方的完整 multiaddr。

## Docker / Coolify

生产部署直接使用 GHCR 的多架构镜像：

```bash
docker run --rm \
  -p 4001:4001/tcp -p 4001:4001/udp \
  -p 4002:4002/tcp -p 4003:4003/udp \
  -v swarm-bootstrap-data:/data \
  -e SWARM_BOOTSTRAP_EXTERNAL_IP=<VPS_PUBLIC_IP> \
  ghcr.io/swarm-apps/swarm-bootstrap:latest
```

`compose.coolify.yml` 可直接导入 Coolify，不需要仓库源码或 Dockerfile。它默认拉取
`ghcr.io/swarm-apps/swarm-bootstrap:latest`；若要固定版本，设置
`SWARM_BOOTSTRAP_IMAGE=ghcr.io/swarm-apps/swarm-bootstrap:0.6.0`。必须持久化 `/data`，
否则身份或 WebRTC 证书改变会让已配置的 bootstrap 地址失效。

本地开发时如需自行构建镜像，可从仓库根目录执行：

```bash
docker build -f crates/bootstrap/Dockerfile -t swarm-bootstrap:local .
```

## 发布

推送 `bootstrap-vX.Y.Z` tag 会触发多平台发布：

- GHCR 镜像 `ghcr.io/swarm-apps/swarm-bootstrap:X.Y.Z`、`X.Y` 与 `latest`（Linux amd64 + arm64 manifest）；
- GitHub Release 的 `x86_64-unknown-linux-gnu` 与 `aarch64-unknown-linux-gnu` 二进制 tarball，以及各自 SHA-256 校验文件。

tag 版本必须与 `crates/bootstrap/Cargo.toml` 的 `version` 完全一致。

为了兼容此前 `swarm-apps/swarm-p2p` 的部署，镜像也会继续发布
`bootstrap-vX.Y.Z`、`bootstrap-vX.Y.Z-amd64` 和 `bootstrap-vX.Y.Z-arm64` 标签。首次迁移时，
需要在已有 GHCR 包的 **Settings → Manage Actions access** 中给 `swarm-apps/SwarmDrop` 写入权限；
若组织策略无法直接授权，则在当前仓库配置具备 `packages:write` 权限的 `GHCR_TOKEN` secret。工作流会优先
使用该 secret，否则使用 `GITHUB_TOKEN`。

## systemd

安装二进制与示例 unit 后，创建 `/etc/swarm-bootstrap.env`：

```ini
SWARM_BOOTSTRAP_EXTERNAL_IP=<VPS_PUBLIC_IP>
SWARM_BOOTSTRAP_KEY_FILE=/opt/swarm-bootstrap/identity.key
SWARM_BOOTSTRAP_WEBRTC_CERT_FILE=/opt/swarm-bootstrap/webrtc.pem
```

再执行：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now swarm-bootstrap
journalctl -u swarm-bootstrap -f
```

## 资源限额

默认最多 128 个 reservation、每 peer 4 个；最多 16 条并发 circuit、每 peer 4 条。
单条 circuit 最长 12 小时，默认不限制字节数，以免大文件传输被 relay 截断。所有值均可通过
同名 `SWARM_BOOTSTRAP_*` 环境变量或 CLI 参数覆盖，执行 `swarm-bootstrap run --help` 查看完整列表。
