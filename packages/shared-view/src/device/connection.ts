/**
 * 链路呈现：把内核给的 transport 判定翻成人能读的一行。
 *
 * 这里只有一个函数，因为链路详情的其余部分（远端地址、中继 PeerId）本来就是
 * 原样展示 + [`shortPeerId`](./organization.ts)，不需要再抽一层。
 */

/**
 * 传输协议的展示名。
 *
 * 全是专有名词，三端一模一样，**也刻意不进任何翻译 catalog**：用户拿这一行去
 * 搜索、去比对日志、去贴 issue，翻成「传输控制协议」只会让它失去用处。
 *
 * 未知值**原样返回**而不是回落到「未知」——后端加了新变体、前端还没跟上时，
 * 显示那个原始字符串远比显示「未知」有用（至少能搜）。
 *
 * 入参用 `string` 而不是联合类型：三端的 transport 由三条 codegen 各自产出
 * （桌面是字符串联合、移动是 `string`），本包不 import 其中任何一份 bindings。
 */
export function transportLabel(
  transport: string | null | undefined,
): string | null {
  if (!transport) return null;
  switch (transport) {
    case "tcp":
      return "TCP";
    case "quic":
      return "QUIC";
    case "webrtc":
      return "WebRTC";
    case "webrtcDirect":
      return "WebRTC Direct";
    default:
      return transport;
  }
}
