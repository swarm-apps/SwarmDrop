/**
 * 信任级别的跨端共用规则。
 *
 * **这里刻意只收「归一 + 判定」，不收默认策略。** 三端的 `defaultReceivePolicy` 都要引用平台
 * 相关的东西——移动要 `resolveReceiveLocation()` 的应用文档目录、要 uniffi 的
 * `MobileReceiveSaveBehavior` 枚举实例；桌面要绝对路径。把它们搬进来就得把平台知识一起搬进来，
 * 违背本包的前提。所以默认策略留在各端，本模块只提供三端都同意的那一小块。
 *
 * 移动端的 `MobileDeviceTrustLevel` 是 uniffi 生成的**运行时枚举对象**，桌面与 Web 则是
 * specta / wasm-bindgen 生成的**字符串字面量联合**。这里以字符串联合为准（它是 wire 上的形态），
 * 枚举 ↔ 字符串的映射留在移动端——那是 uniffi 的细节，不该外溢。
 */

/** 信任级别的规范形态，与 Rust 的 `DeviceTrustLevel` serde 表示一致。 */
export type TrustLevel = "owned" | "collaborator" | "temporary" | "blocked";

/**
 * 供选择器渲染的固定顺序：从最信任到最不信任。
 *
 * 三端的信任级别下拉都该按这个顺序排——顺序本身是产品语义（用户在读一条从松到紧的梯度），
 * 各端各排一遍迟早会不一致。文案不在这里，由各端 i18n 提供。
 */
export const TRUST_LEVELS: readonly TrustLevel[] = [
  "owned",
  "collaborator",
  "temporary",
  "blocked",
];

/**
 * 缺省信任级别 —— 内核允许 `trustLevel` 为空（旧配对记录、尚未表态的设备），
 * UI 一律按「协作者」呈现：既不是完全信任，也不至于拦住正常传输。
 */
export function normalizeTrustLevel(level: TrustLevel | null | undefined): TrustLevel {
  return level ?? "collaborator";
}

/**
 * 能否向这台设备发送。
 *
 * 两个条件缺一不可：在线，且未被阻止。**离线判定必须在 UI 层做**，不能只靠内核拒绝——
 * 给一个必然失败的目标亮着发送按钮，用户点完只收到一条报错（PRODUCT.md 原则 2·状态诚实可见）。
 */
export function canSendToDevice(device: {
  status: string;
  trustLevel?: TrustLevel | null;
}): boolean {
  return device.status === "online" && normalizeTrustLevel(device.trustLevel) !== "blocked";
}

/**
 * 一台设备当前接收策略的一句话归纳，供徽标 / 摘要行选文案。
 *
 * 返回的是**判别式而非文案**：文案要翻译，本包没有 i18n 运行时。各端拿这个 key 去查自己的
 * catalog，于是「什么情况算自动接收」这条判定只有一份，而三端的说法各自本地化。
 */
export type PolicyNote = "auto_accept" | "manual_confirmation" | "temporary" | "blocked";

export function policyNoteFor(
  level: TrustLevel,
  policy: { autoAccept: boolean; requireConfirmation: boolean },
): PolicyNote {
  if (level === "blocked") return "blocked";
  if (level === "temporary") return "temporary";
  // 两个开关同时成立才算真自动：`requireConfirmation` 会把 `autoAccept` 顶掉。
  return policy.autoAccept && !policy.requireConfirmation ? "auto_accept" : "manual_confirmation";
}
