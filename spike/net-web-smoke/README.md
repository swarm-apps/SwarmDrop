# net-web-smoke —— swarmdrop-net 浏览器冒烟（M5）

验证新内核在真实浏览器里的三条通路（对应重构计划 M5 验收）：

| # | 通路 | 验证点 |
|---|---|---|
| ① | 浏览器 → helper 的 `ws://`（私网 IP） | 私网豁免 mixed content；Browser preset 可 dial |
| ② | 浏览器 → helper 的 `webrtc-direct`（certhash） | 免域名免 CA 的公网/私网入口 |
| ③ | 浏览器 reserve + native 拨 circuit 地址反向 echo | **浏览器被动接收连接**（circuit listen）+ 双向 RPC |

共享 `proto` 模块两端同一份（协议/RPC/Endpoint API 零 cfg）——「rust-wasm 单核心包」的直接证据。

## 步骤

```sh
cd spike/net-web-smoke

# 1. native helper（relay server + ws + webrtc-direct + echo 服务）
cargo run -- helper
#    → 记下打印的 node id 与 ws / webrtc-direct 监听地址

# 2. 构建 wasm（macOS 需 brew install llvm，见 .cargo/config.toml）
wasm-pack build --target web --weak-refs --release --out-dir static/pkg

# 3. 起静态服务器并开浏览器
python3 -m http.server 8080 -d static
#    → http://<本机私网IP>:8080 （用私网 IP 而非 localhost，才测得到私网豁免那格）

# 4. 页面里依次：
#    ① addr 填 helper 的 ws 地址（带 /p2p/<id>）→ connect
#    ② 同地址 → reserve → 拿到 circuit 地址
#    ③ peer 填 helper 的 node id → echo（浏览器 → native）
#    ④ 另开终端：cargo run -- dial "<circuit-addr>"（native → 浏览器，验证被动接收）
#    ⑤ addr 换 webrtc-direct 地址重复 ①③
```

## 验证记录

| 日期 | 环境 | ① ws | ② webrtc-direct | ③ circuit 反向 echo | 备注 |
|---|---|---|---|---|---|
| 2026-07-18 | macOS + Chromium（自动化）；helper 与浏览器同机，私网 192.168.50.105；http:// 页面 | ✅ path=Local | ✅ certhash 直拨 + RPC echo | ✅ reserve Active → native 拨 circuit path=Relayed，浏览器 RPC handler 应答 | wasm 产物 1836KB 裸 / **598KB gzip**（对照 iroh spike 849KB gzip，小 30%）；浏览器 Endpoint 建立即时；已连接 peer 的 connect 幂等返回快照（语义正确）。**未测格**：https 页面（mixed content 豁免那格 spike/webrtc-direct-https 已实证）、跨机器、Safari/Firefox |
