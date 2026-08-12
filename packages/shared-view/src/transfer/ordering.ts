/**
 * 传输列表的排序 —— 三端唯一的一份。
 *
 * ## 为什么是「最后活动时间」而不是「开始时间」
 *
 * 用户在这张列表上问的是「最近发生了什么」。一条三天前开始、今天刚续传的会话，按开始
 * 时间排会沉回三天前的位置——而它恰恰是此刻最该被看到的那条。暂停后隔天恢复是常规操作，
 * 不是边角情况。
 *
 * ⚠️ **不要以为活跃会话会自己浮顶。** DB 里 `updated_at` 确实随每次 checkpoint 刷新，
 * 但那个事实到不了前端：checkpoint 只发 `TransferProgress`，`TransferProjection` 只在
 * 状态转换时发（`crates/transfer/src/coordinator.rs`），而三端 store 没有任何一处从
 * 进度事件写 `updatedAt`。所以一条跑了 20 分钟的传输，它的 `updatedAt` 冻在「进入
 * active 那一刻」，期间别的会话结束就会排到它上面。这是本排序的已知取舍——活跃行
 * 靠「唯一带进度条、唯一在动」被认出来，不靠位置。判据与取舍见 DESIGN.md。
 *
 * ## 为什么不按状态分层
 *
 * 三端曾各自先按状态分组再排时间（桌面三档隐式、移动四组 SectionList、Web 两段），
 * 于是「最近发生了什么」在任何一端都读不出来：一条刚失败的会话会排在一堆几天前的完成
 * 记录之后。分组想解决的「找到需要处理的那条」由筛选承担——那是用户主动发起的动作，
 * 而不是每次打开列表都要付的阅读成本。判据写在 `DESIGN.md` 的
 * **Transfer List Order Contract**。
 *
 * ## 为什么收进本包
 *
 * 收进来之前三端各写一份，且已经漂了：桌面按 `startedAt`、移动与 Web 按 `updatedAt`，
 * 同一条续传会话在桌面沉底、在另两端置顶。这正是 README 判据 3（输出跨端一致）说的
 * 那种「必须改掉其中一端渲染输出」的分歧。
 */

/**
 * 排序需要的最小结构。
 *
 * 三端的 projection 类型各不相同，且**移动端的 `updatedAt` 是 `bigint`**（uniffi 的
 * i64 映射），所以这里不能收窄成 `number`。跨类型混用的后果见 `compareByTimelineDesc`
 * 的实现注释——那是本比较器唯一会静默出错的地方。
 */
export interface TimelineOrdered {
  readonly sessionId: string;
  readonly updatedAt: number | bigint;
}

/**
 * 最后活动时间倒序；同一毫秒的按 `sessionId` 兜底。
 *
 * 兜底不是洁癖：列表源是 `Object.values(projections)`，两条同毫秒记录的相对位置会随
 * Record 的插入序变，表现为「刷新一下这两行就换个位置」。
 */
export function compareByTimelineDesc(a: TimelineOrdered, b: TimelineOrdered): number {
  // 先归一成 number，**不要**直接拿 `updatedAt` 比。`100n !== 100` 恒为 true（严格比较
  // 跨类型不相等），而 `100n < 100` 又按数值判 false——两条组合起来会让
  // `compare(a,b)` 与 `compare(b,a)` 同时返回 -1，是个不满足反对称性的比较器，
  // TimSort 拿到它会给出依赖输入顺序的任意结果，正好毁掉下面那行兜底想要的确定性。
  // 单端内类型本来一致，但签名允许混用（乐观插入一行 `Date.now()` 就够了），而这种
  // 错法是**静默**的——旧的 `Number(b.updatedAt - a.updatedAt)` 至少会抛 TypeError。
  // 毫秒时间戳 ~1.7e12，远在 `MAX_SAFE_INTEGER` 之内，转换无损。
  const at = Number(a.updatedAt);
  const bt = Number(b.updatedAt);
  if (at !== bt) return at < bt ? 1 : -1;
  return a.sessionId < b.sessionId ? -1 : a.sessionId > b.sessionId ? 1 : 0;
}

/** 按时间线倒序排列，不改原数组。 */
export function sortByTimelineDesc<T extends TimelineOrdered>(items: readonly T[]): T[] {
  return [...items].sort(compareByTimelineDesc);
}
