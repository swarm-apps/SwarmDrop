import type { ReactNode } from "react";

/**
 * 首页的排版原语。
 *
 * 存在的理由只有一个：**让「区块之间的节奏」变成一个能改的数**，而不是散在八个区块里的
 * 八组 `py-*`。此前每个区块各写各的 padding，于是页面读起来是等距堆叠——
 * DESIGN.md 的 Layout Density Contract 说的「间距编码分组」在纵向上同样成立。
 *
 * 这里**不放视觉装饰**（没有卡片、没有图标片、没有 eyebrow 容器）。装饰属于各区块自己，
 * 抽到这里会变成「每个区块长得一样」的模板——正是本次重写要解决的问题。
 */

/** 全站内容列宽。与文档区一致，也与应用区的 1240 是同一量级。 */
export const SHELL = "mx-auto w-full max-w-6xl px-6";

/**
 * 区块外壳。`tone` 决定底色层次——**只有三档，且都在当前主题内**，不做明暗反转
 * （整页只有 hero 一处是恒定深色，那是刻意的单点，见 `page.tsx`）。
 */
export function Section({
  id,
  tone = "plain",
  className = "",
  children,
}: {
  id?: string;
  tone?: "plain" | "sunken" | "brand";
  className?: string;
  children: ReactNode;
}) {
  const toneClass =
    tone === "sunken"
      ? "border-y border-fd-border bg-fd-card/30"
      : tone === "brand"
        ? "bg-[var(--brand-solid)] text-[var(--brand-ink)]"
        : "";
  return (
    <section id={id} className={`${toneClass} ${id ? "scroll-mt-20" : ""} ${className}`}>
      {/* 区块纵向留白比常见落地页宽一档（96 / 128px）。这一页的信息密度不低——
          八个区块、三张大图、一张定义列表——留白是它唯一的换气口。压回 py-16 会让整页
          读成一份规格书。 */}
      <div className={`${SHELL} py-24 sm:py-32`}>{children}</div>
    </section>
  );
}

/**
 * 区块标题。
 *
 * **刻意没有 eyebrow 参数**：小号全大写字距标签放在每个区块标题上方，是这类页面最容易
 * 被认出的模板痕迹——旧版首页四个区块用了四枚。标题自己说得清楚，位置本身就是分类。
 *
 * `lead` 与标题**竖着叠**，不做「左大标题 + 右小段落」的分栏页头：那个版式让一个区块
 * 同时有两个重心，而每个区块只该有一句话。
 */
export function SectionHead({
  title,
  lead,
  className = "",
}: {
  title: ReactNode;
  lead?: ReactNode;
  className?: string;
}) {
  return (
    <div className={`reveal ${className}`}>
      <h2 className="text-balance text-[clamp(1.75rem,3.4vw,2.6rem)] font-bold leading-[1.15] tracking-[-0.025em]">
        {title}
      </h2>
      {lead && (
        <p className="mt-4 max-w-[52ch] text-pretty text-[15px] leading-7 text-fd-muted-foreground sm:text-base">
          {lead}
        </p>
      )}
    </div>
  );
}

/**
 * 机器值。The Mono Truth Rule 的落地：ID、地址、哈希、字节数一律等宽 + tabular-nums。
 *
 * 首页上它同时是**质感的来源**——这个产品的可信度来自「把真实链路摊开给你看」，
 * 而不是来自一句「军工级加密」。所以页面上出现的地址都是从一台真实运行的节点上取的，
 * 只做截断，不编造。截断必须留 `…`（DESIGN.md：截断的地址要看得出被截过，
 * 否则它仍然像一条完整的 multiaddr，会被原样粘进 issue）。
 */
export function Mono({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <code
      className={`font-mono text-[12.5px] leading-5 tabular-nums break-all text-fd-muted-foreground ${className}`}
    >
      {children}
    </code>
  );
}
