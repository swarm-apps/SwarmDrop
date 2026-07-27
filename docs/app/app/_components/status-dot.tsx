// 状态点：第三次出现同一渲染模式后抽出（node-status-pill / connection-panel / device-list
// 各自都要画一个「圆点+文字」的语义状态——颜色是状态语义编码，不是装饰（DESIGN.md one-accent
// 规则外的例外）。容器（pill / box / 内联）各处不同，故只抽最小公分母的圆点本身。

export function StatusDot({ colorClass, pulse = false }: { colorClass: string; pulse?: boolean }) {
  return (
    <span
      className={`size-1.5 shrink-0 rounded-full ${colorClass} ${pulse ? "animate-pulse motion-reduce:animate-none" : ""}`}
    />
  );
}
