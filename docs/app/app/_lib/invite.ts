// 邀请相关的前端常量。

/**
 * 邀请有效期（秒），与 `crates/invite/src/invite.rs` 的 `INVITE_TTL_SECS` 一致。
 * **改这里必须同步那个 Rust 常量**——桌面端 `src/stores/pairing-store.ts` 与移动端
 * `mobile/src/stores/pairing-invite-store.ts` 各自也留了一份**同名**副本，同样的约定。
 *
 * 同名同单位是刻意的：这类跨语言复制唯一的兜底就是 `grep INVITE_TTL_SECS` 能一次捞齐
 * 所有副本，起个别名等于把自己从那次 grep 里摘出去。
 */
export const INVITE_TTL_SECS = 86_400;

/** 文案用的小时数，从上面派生——别在句子里直接写「24」，TTL 一改那些数字就开始骗人。 */
export const INVITE_TTL_HOURS = INVITE_TTL_SECS / 3600;
