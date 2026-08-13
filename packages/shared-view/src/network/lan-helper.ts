/**
 * 局域网协助地址：把内核下发的私网监听地址，变成「浏览器可以直接拨的那几条」。
 *
 * 桌面与移动此前各有一份逐行同构的实现，而两份都是白名单式的三元链——
 * 加一种传输不会有任何编译错误，只会被静默丢掉。WebTransport 上线后两端因此都只显示
 * webrtc-direct，用户复制走的是慢 4.5 倍的那条，而且**没有别的途径拿到快的那条**。
 */

/**
 * 浏览器**拨得动**的传输，**按吞吐从好到差排列**。
 *
 * 顺序即偏好，与 Rust 侧 `DialTier` 同源：回环实测 WebTransport 322 MiB/s vs
 * webrtc-direct 72 MiB/s（且后者方差大一个数量级）。协助节点是**中继**——用户经它
 * 中转的每一个字节都走这条腿，所以这里排第一的那条就是实际吞吐的上限。
 *
 * # 两条维护纪律
 *
 * - **判定按段落特异性**：WebTransport 地址形如 `…/udp/…/quic-v1/webtransport/certhash/…`，
 *   将来若新增 `/quic` 一项，必须排在 `/webtransport` **之后**，否则前者永远匹配不到。
 * - **表驱动、不写条件链**：新增传输时只需在表里加一行；而条件链的失效方式是「加了新传输
 *   却谁也没发现」——本文件的前身正是这么把 WebTransport 丢掉的。
 *
 * WebSocket 已于 2026-07-28 整体移除（transport、桌面 listener、bootstrap 端口），
 * 故表里没有它。此前两端都还留着一个永远匹配不到的 `/ws` 分支。
 */
const BROWSER_DIALABLE = [
  { segment: "/webtransport", label: "WebTransport" },
  { segment: "/webrtc-direct", label: "WebRTC Direct" },
] as const;

/** 协助地址的传输名。**是专有名词，永不翻译**（同 `transportFromAddr`）。 */
export type LanHelperTransport = (typeof BROWSER_DIALABLE)[number]["label"];

export type LanHelperAddress = {
  /** 可直接复制粘贴的完整 multiaddr（已补 `/p2p/<peerId>`）。 */
  address: string;
  transport: LanHelperTransport;
};

/**
 * 从内核下发的协助地址里，挑出浏览器可拨的那些，并补上 peer id。
 *
 * 返回顺序**就是推荐顺序**——调用方直接按序展示即可，第一条是最快的。
 *
 * `peerId` 缺失时返回空：不带 `/p2p/` 的地址对端拨不了，给出去只会让人复制一条坏地址。
 */
export function lanHelperAddresses(
  addrs: readonly string[],
  peerId?: string,
): LanHelperAddress[] {
  if (!peerId) return [];

  return BROWSER_DIALABLE.flatMap(({ segment, label }) =>
    addrs
      // 协助地址本该是监听地址，circuit 混进来只可能是上游筛漏了。放行的后果是
      // 用户复制到一条「经另一个中继再中继」的地址——多一跳，正是这个功能要避免的。
      .filter((addr) => addr.includes(segment) && !addr.includes("/p2p-circuit"))
      .map((addr) => ({
        address: addr.includes("/p2p/") ? addr : `${addr}/p2p/${peerId}`,
        transport: label,
      })),
  );
}
