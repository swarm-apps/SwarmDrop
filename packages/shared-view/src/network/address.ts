/**
 * multiaddr 的**呈现**逻辑：传输名与短形式。纯字符串处理，零平台依赖。
 *
 * 桌面与 Web 此前各有一份逐行同构的实现，而它们的判定顺序是「加一个传输就会静默出错」
 * 的结构（见下），却只有 Web 那份有测试。合并到这里之后两端共用同一份、同一组测试。
 *
 * **三端都吃**：桌面与 Web 的引导节点列表、移动端的 `InfraLinkRow`。移动端那份此前带着
 * 同样两个缺陷（`indexOf` + 前缀截断不补省略号），只是屏幕窄、阈值取得更小 —— 宽度因此
 * 做成参数，逻辑只有这一份。
 */

/**
 * 传输名。**是专有名词，永不翻译**（DESIGN.md 的 Device Card Contract：用户会搜它、
 * 贴进 issue、跟日志比对）。
 *
 * ⚠️ **判定顺序按特异性从高到低，这是一条没有编译期保护的不变量**：
 *
 * - `/webrtc-direct` 必须排在 `/webrtc` 前面，否则前者永远匹配不到；
 * - `/webtransport` 必须排在 `/quic` 前面 —— WebTransport 地址形如
 *   `/udp/4004/quic-v1/webtransport/certhash/…`，**两个段同时存在**。
 *
 * 排错了不会编译失败，症状是地址被标成另一种传输，而同屏的校验又以
 * `unsupportedTransport` 拒掉它 —— 用户看到两句互相矛盾的话。`address.test.ts` 是
 * 这条顺序的唯一看守。
 *
 * `WebRTC` 与 `WebRTC Direct` 是**两种传输**（前者要打洞与信令，后者是直接拨公网裸 IP），
 * 桌面那份曾把后者也标成 "WebRTC" —— 混称会让人以为换个 relay 也能用打洞地址。
 */
export function transportFromAddr(addr: string): string {
  if (addr.includes("/webrtc-direct")) return "WebRTC Direct";
  if (addr.includes("/webrtc")) return "WebRTC";
  if (addr.includes("/webtransport")) return "WebTransport";
  if (addr.includes("/quic")) return "QUIC";
  if (addr.includes("/tcp/")) return "TCP";
  return "P2P";
}

/**
 * 地址的短形式：保留协议头与末尾 peer id，中段以 `…` 省略。
 *
 * `maxLength` 只影响宽度，判据两端一致（移动端屏幕窄，传 46）。
 *
 * 两条判据都不能退回去：
 *
 * - **取末位的 `/p2p/`（`lastIndexOf`）。** circuit 地址形如
 *   `…/p2p/<relay>/p2p-circuit/p2p/<target>`，用 `indexOf` 会命中**中转**那个，于是
 *   `/p2p-circuit/` 整段被静默吃掉 —— 一条中继地址被渲染成直连地址，而那串"peer id"
 *   是两个不同 peer 的头尾拼接，不属于任何一方。
 * - **前缀被截时补省略号。** 不补的话输出是
 *   `/ip4/47.115.172.218/udp/4003/w/p2p/12D3Ko…1utep`：certhash 整段消失、切口处毫无
 *   提示，看起来像一条完整但写错的 multiaddr，而用户会照着它去 issue 里贴。
 */
export function truncateAddr(addr: string, maxLength = 60): string {
  if (addr.length <= maxLength) return addr;
  const prefixCap = Math.floor(maxLength / 2);
  const p2pIdx = addr.lastIndexOf("/p2p/");
  if (p2pIdx === -1) {
    return `${addr.slice(0, prefixCap)}…${addr.slice(-12)}`;
  }
  const prefix = addr.slice(0, p2pIdx);
  const peer = addr.slice(p2pIdx + 5);
  const shortPeer =
    peer.length > 16 ? `${peer.slice(0, 6)}…${peer.slice(-5)}` : peer;
  const shortPrefix =
    prefix.length > prefixCap ? `${prefix.slice(0, prefixCap)}…` : prefix;
  return `${shortPrefix}/p2p/${shortPeer}`;
}
