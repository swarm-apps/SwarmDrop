# 设计：局域网直连升级与链路可观测

## 决策 1：升级的地址来源以 identify 为主，mDNS 为辅

直觉的修法是「把 mDNS 修好」。但 mDNS 在两个移动平台上都要过平台的门（iOS 本地网络权限 +
可能的 multicast entitlement、Android MulticastLock），而**对端的私网监听地址本来就会经 identify
自报**——那条路径不碰组播，一个平台门都不用过。

所以主路径是 identify，mDNS 只是「来得更早的那份」（identify 默认 5 分钟一轮，mDNS 通常几秒内到）。
两个来源汇进同一个 `try_upgrade_to_lan`。

**安全性**：拨的是已过 Noise 认证的对端自报的地址，libp2p 握手时校验 PeerId——拨到网段内其他机器
只会握手失败。这也不新增信任面：`is_lan_discovered`（`PairingMethod::Direct` 的唯一授权依据）读的
仍然只有 mDNS 来源的地址，`try_upgrade_to_lan` 一个字节都不往那张表里写。`device_manager` 里
`PeerInfo.addrs`（mDNS 证据）与新加的 `conn_addr`（链路快照）**分开存**，正是为了这条不能混。

## 决策 2：LAN 升级不做 `should_initiate` 定序，打洞继续做

打洞那边定序是对的：一次尝试是数秒 ICE + 信令往返，两端同时发起会白建一条连接。

LAN 不一样：同网段握手是毫秒级、没有信令往返。两端各拨一次，最坏只是多一条 60s 后被 idle 回收的
连接。而定序会让「只有一端拨得通」——一端防火墙拦入站、一端 mDNS 瞎了——这类局域网里很常见的
情况彻底没救（不发起的那端会一直等，而对端根本发不起来）。

代价与收益的比值在两条路径上差一个数量级，所以规则不同。

## 决策 3：两条升级路径的在途标记必须分开存

原本只有一个 `upgrading: HashSet<PeerId>`。若 LAN 与打洞共用它，跨网场景会死锁：

1. 对端在自己家 Wi-Fi，identify 自报 `192.168.1.x`；
2. 本端在另一个网络，拨它必然失败；
3. 失败清标记 → 下一轮 identify 又先试 LAN → 又失败……
4. **打洞永远排不上队**，而 identify 默认 5 分钟才来一轮。

所以拆成 `upgrading_lan` / `upgrading_direct`，两条路径在 identify 到达时**都发起、互不阻塞**。
LAN 那条通常毫秒级就赢，打洞那条随后建立的连接会因 `path_rank` 落选、idle 后回收——多一条短命
连接，换的是「LAN 拨不通时不用再等 5 分钟才轮到打洞」。

## 决策 3.5：候选上限按传输分组，不能笼统取前 N 个

第一版写的是 `.take(4)`，理由是「对端可能自报一长串地址，全拨会把一次升级变成对内网的批量
探测」。理由本身没错，实现方式是错的。

原生端 preset 同时监听 **tcp / quic-v1 / webrtc-direct** 三种，各自再乘以网卡数与 IPv4/IPv6
ULA——一台手机自报六条私网地址是常态。而 `webrtc-direct` 是 listen 列表里**最后**注册的，
它的地址排在末位，`take(4)` 截掉的正是它。

那一刀砍在最要紧的地方：**浏览器拨不了裸 TCP/QUIC，webrtc-direct 是它够到局域网内原生端的
唯一路径**。截掉它，「浏览器 ↔ 同网段的手机/桌面」这一格永远停在中继——而症状是纯粹的
「就是不升级」，没有任何报错可查。

改为按 `Addr::transport()` 分组、每种各留 2 个：既保证每种传输有代表，也仍然挡得住地址风暴。
两条单测钉死（保留每种传输 / 单种传输仍受限）。

顺带说明浏览器为什么不会因为拨到 tcp/quic 而受损：libp2p 对不支持的地址返回
`MultiaddrNotSupported` 并立即跳到下一个候选，不占用时间，只有**全部**候选失败才算这次 dial 失败。

## 决策 4：`Addr::transport()` 必须先判 WebRTC

打洞地址形如 `<relay>/p2p-circuit/webrtc/p2p/<peer>`：`/webrtc` 在 circuit 段**之后**，而前半段是
到 relay 的 `/tcp` 或 `/quic-v1`。按协议栈顺序找会把打洞连接报成 TCP——数据面明明一个字节不过中继。

同一个陷阱在 `classify_path` 里也有（那里的解法是 `is_hole_punched` 排在 `relayed` 之前）。两处
现在都由单测钉死。

纯 circuit 地址（无 `/webrtc`）返回的是**承载中转字节的那条连接**的传输，即本端 ↔ relay 之间的
TCP/QUIC——那正是排障要看的东西。

`None` 是真实存在的返回值：入站中继连接的 `send_back_addr` 只有 `/p2p/<src>` 一段，libp2p 就是
这么填的，地址里没有任何传输信息。呈现层照实显示「未知」，不编默认值。

## 决策 5：`TransportKind` 放 net-base，host 直接复用（不再抄一份）

`PathKind` → `ConnectionType` 那条既有映射是**产品层重新表述内核概念**，值得两个枚举。
`TransportKind` 不是——它在内核与产品层是同一个意思，抄一份只会得到两个永远同步的孪生枚举。

代价是给 `crates/net-base` 加一个 **默认关闭** 的 `specta` feature（桌面 IPC 导出用）。移动端与
wasm 都不开它，「依赖极小」的底座约束仍然成立。

## 决策 6：`ConnectionSnapshot` —— 四个字段必须一起产出

`device_manager` 有两条构造 `Device` 的分支（`DeviceFilter::Paired` 与 `peer_to_device`），此前各自
拼 `(status, connection, latency)`。加上 `details` 后，分开算会配出「显示局域网直连，详情却是一条
早已失效的 circuit 地址」这类互相矛盾的组合。

收成一个 `ConnectionSnapshot` 后，「在线 / 离线 / 在线但内核无记录」三种情形各是一个构造函数，
两条分支都只能整份取用。

**断连宽限期的降级是刻意不对称的**：`connection` 回退到 mDNS 地址推断（局域网设备据此仍显示 LAN），
`details` 直接为 `None`——链路已经没了，给出旧地址只会让人对着一条失效的连接排查。

## 决策 7：mDNS 初始化失败降级为 warn

`Behaviour::new` 里原本是 `.expect("mDNS initialization failed")`。mDNS 是可选的发现加速手段，
不是必需品：绑不上 5353（iOS 上被 mDNSResponder 占着、容器/无线网卡缺组播接口）只该退化成
「局域网设备发现得慢一点」。

此前那行等于把一个平台可选能力做成了启动的硬前提——任何不给绑 5353 的环境都会在节点启动时
直接 panic。而且退化后局域网直连并没有丢：决策 1 的 identify 路径不碰组播。

## 风险：iOS 的 multicast entitlement（未验证）

`NSLocalNetworkUsageDescription` + `NSBonjourServices` 是**必要**条件，不一定**充分**。

iOS 14 起，通过系统 Bonjour API（`NWBrowser` / `NetService`）浏览 mDNS 不需要额外授权，但
**直接 bind 5353 并加入 `224.0.0.251` 组播组的裸 socket**（libp2p-mdns 正是这么做的）可能还需要
Apple 特批的 `com.apple.developer.networking.multicast` entitlement。本轮没有真机 + 开发者账号可验。

**这个风险不阻塞本变更的价值**：LAN 直连由决策 1 的 identify 路径承担，mDNS 只影响「多快发现」。
若真机实测确认 iOS 组播不通，补申请 entitlement 即可，届时无需改任何代码。

## 待真机验证

- iOS：本地网络权限弹窗是否出现；libp2p mDNS 是否真的收发到组播。
- Android：MulticastLock 生效后是否能在多播域看到对端。
- 两端：relay 连接建立后，identify 到达时是否观察到 `upgrading relayed connection to lan direct`
  的 info 日志，以及随后的 `PathChanged` → `Local`。
