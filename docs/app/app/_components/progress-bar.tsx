// track + fill 两层结构的进度条。发送面板（prepare 阶段 hash 进度）与传输活动面板
// （会话字节进度）此前各写了一份逐字节相同的实现。

export function ProgressBar({ percent, className = "" }: { percent: number; className?: string }) {
  return (
    <div className={`h-1.5 overflow-hidden rounded-full bg-fd-border ${className}`}>
      <div
        className="h-full bg-[var(--brand-solid)] transition-[width]"
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}
