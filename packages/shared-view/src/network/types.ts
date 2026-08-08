/**
 * 网络判据的结构化入参。
 *
 * 本包不 import 任何一端的 bindings（README 的「类型边界」），所以这里只声明判据
 * **真正读到的字段**，三端各自的 codegen 产物都能结构化赋值。
 *
 * 字面量拼写逐字沿用内核的 serde 表示（`camelCase`）——桌面与 Web 的 codegen 产出
 * 的正是同一组字符串联合，直传即可；需要映射的只有 uniffi 那一端。
 */

/** 单条基础设施关系的 relay 轨道状态（内核 `RelayLinkState` 的投影）。 */
export type InfraRelayState =
  | { readonly kind: "connecting" }
  | { readonly kind: "active"; readonly circuitAddr: string }
  | { readonly kind: "failed"; readonly lastError: string };

/**
 * 当前不参与 relay 收敛的原因（内核 `InfraExclusion` 的投影）。
 *
 * 判别码只读 `kind`，故声明成开放的 `string`：后端加变体、前端还没跟上时，
 * 判据仍然把它当「被设置排除」处理（中性 + 去设置），这比崩到「故障」档正确。
 */
export interface InfraExclusionView {
  readonly kind: string;
}

/** 判据用得到的那部分 `InfraLink`。 */
export interface InfraLinkView {
  readonly roles: { readonly relayServer: boolean; readonly kadServer: boolean };
  readonly scope: "public" | "lan";
  /**
   * 首次进入候选表的时刻（毫秒）。
   *
   * 三端的 codegen 给的形状不同（桌面/Web 是 ISO 串、移动是 i64），**转换归调用点**
   * ——本包不设适配层，那会把「哪一端叫什么」带进平台中立的包。
   */
  readonly firstSeenMs: number;
  readonly connected: boolean;
  readonly relay: InfraRelayState | null;
  readonly everActive: boolean;
  readonly excluded: InfraExclusionView | null;
}

/**
 * 判据用得到的那部分 `NetworkStatus`。
 *
 * **没有 `publicAddr`**：可达性判定含 `CandidateScope` 这条领域知识，且 `crates/web` 是
 * 第四个消费者（生成邀请时判本机可达），TS 包救不到它——所以规则留在 Rust，这里只消费
 * 它算好的 `publicReachable`。把 `publicAddr` 声明进来会引诱人在 TS 里重算一遍。
 */
export interface NodeStatusView {
  readonly status: string;
  readonly publicReachable: boolean;
  readonly connectedPeers: number;
}
