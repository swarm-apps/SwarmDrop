/**
 * 按「今天 / 昨天 / 本周内 / 更早」把带时间戳的条目分桶。
 *
 * ## 为什么它能进这个包
 *
 * 它返回的是**判别式 key，不是文案**（`"today"` 而不是「今天」）——本包的归属判据第 3 条
 * 判出去的是「输出即各端有意不同的本地化文案」那一类，而分桶的边界（今天从几点开始、
 * 本周算几天）是纯逻辑，三端不该有不同答案。文案由各端拿 key 查自己的 catalog。
 *
 * 收进来的直接动因：收件箱的时间分组桌面有、Web 没有，而它是「一屏几十条时怎么找到
 * 昨天那个文件」的唯一结构。在 Web 再抄一份就有了两个会各自漂移的边界定义。
 *
 * ## 边界用**日历运算**，不是「减去 86400 秒」
 *
 * `new Date(y, m, d - 1)` 让 JS 自己处理跨月、跨年与**夏令时**；用毫秒减法则会在 DST
 * 切换的那一天把「昨天」算成 23 小时或 25 小时前，边界上的条目落错桶。
 */

/** 分桶键。渲染顺序即声明顺序。 */
export type TimeBucketKey = "today" | "yesterday" | "week" | "earlier";

/** 渲染顺序的单一定义——各端不要自己再排一遍。 */
export const TIME_BUCKET_ORDER: readonly TimeBucketKey[] = [
  "today",
  "yesterday",
  "week",
  "earlier",
];

export interface TimeBucket<T> {
  key: TimeBucketKey;
  items: T[];
}

/**
 * @param items 已按需要排好序的条目——本函数**不排序**，只分桶并保持原有相对顺序。
 * @param timestampOf 取毫秒时间戳。取不到有意义的值（`NaN` 与 `±Infinity`，判据是
 *   `Number.isFinite`）时落进 `earlier`，而不是抛错或丢弃：一条时间坏掉的记录仍然是
 *   用户的数据，得让它出现在某个地方。`Infinity` 同样要拦——它会静默通过所有
 *   `>=` 比较，把一条坏数据顶到「今天」的最前面。
 * @param now 参考时刻（毫秒）。可注入，测试才不必依赖真实时钟。
 */
export function groupByTimeBucket<T>(
  items: readonly T[],
  timestampOf: (item: T) => number,
  now: number = Date.now(),
): TimeBucket<T>[] {
  const ref = new Date(now);
  const y = ref.getFullYear();
  const m = ref.getMonth();
  const d = ref.getDate();
  const startOfToday = new Date(y, m, d).getTime();
  const startOfYesterday = new Date(y, m, d - 1).getTime();
  // 「本周内」= 含今天在内的最近 7 天，不是自然周——用户问的是「最近」，
  // 而自然周会让周一早上的列表突然只剩「今天」。
  const startOfWeek = new Date(y, m, d - 6).getTime();

  const buckets: Record<TimeBucketKey, T[]> = {
    today: [],
    yesterday: [],
    week: [],
    earlier: [],
  };

  for (const item of items) {
    const ts = timestampOf(item);
    if (!Number.isFinite(ts)) buckets.earlier.push(item);
    else if (ts >= startOfToday) buckets.today.push(item);
    else if (ts >= startOfYesterday) buckets.yesterday.push(item);
    else if (ts >= startOfWeek) buckets.week.push(item);
    else buckets.earlier.push(item);
  }

  // 空桶不出现：一个只有标题没有内容的分组头是纯噪音。
  return TIME_BUCKET_ORDER.filter((key) => buckets[key].length > 0).map((key) => ({
    key,
    items: buckets[key],
  }));
}
