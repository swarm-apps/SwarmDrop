# 网络

libp2p 网络层的问题修复记录（relay circuit 等）。当前内核见 [network-kernel/](../network-kernel/)。

- [三端到底怎么连上：ws、webrtc-direct 与中继的真实分工](2026-07-cross-end-connectivity.md)
  —— 破除四个误解（transport ≠ 拓扑、direct ≠ 穿 NAT、扫码即信令、Android 无 ws），
  给出完整连通性矩阵与「方向不对称」这条产品约束。**搞不清哪条腿走哪里时先读这篇。**
- [公网 Relay 接入浏览器：把“能拨通”变成“能被拨通”](2026-07-public-relay-and-browser-entry.md)
  —— 多端地址清单、持久化 WebRTC 证书、端口暴露、reservation 与部署验收。

想知道 webrtc-direct **内部**怎么工作（为什么服务端要关掉指纹校验、certhash 为什么必须
持久化、为什么还要再跑一次 Noise），见 [webrtc/](../webrtc/) 系列——它从零讲 WebRTC，
并复盘自研传输时向上游提的六个补丁。
