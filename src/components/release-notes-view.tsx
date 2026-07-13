// release-notes-view —— release notes 渲染槽。缺省用 react-markdown 渲染 Markdown
// (标题/加粗/列表/链接/代码均按真实 markdown 呈现),用显式 components 映射到 Tailwind token
// 样式(不依赖 @tailwindcss/typography 插件,保持 registry 自足)。可通过 `renderer` 覆盖为
// 自定义渲染器。背景 / 圆角由各父组件负责,本组件只管滚动区。registry:component。

import type { ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import { cn } from "@/lib/utils";

export interface ReleaseNotesViewProps {
  notes?: string;
  /** 自定义渲染(如接自定义 Markdown 渲染器);缺省用 react-markdown 渲染 Markdown。 */
  renderer?: (notes: string) => ReactNode;
  className?: string;
}

// 只透传 children(必要时 href):按元素映射到 token 样式,标题降到与紧凑 UI 一致的字号。
const components: Components = {
  h1: ({ children }) => (
    <h3 className="mt-3 mb-1 text-sm font-semibold text-foreground first:mt-0">{children}</h3>
  ),
  h2: ({ children }) => (
    <h3 className="mt-3 mb-1 text-sm font-semibold text-foreground first:mt-0">{children}</h3>
  ),
  h3: ({ children }) => (
    <h4 className="mt-2 mb-1 text-[13px] font-semibold text-foreground first:mt-0">{children}</h4>
  ),
  p: ({ children }) => <p className="my-1 leading-relaxed">{children}</p>,
  ul: ({ children }) => <ul className="my-1 list-disc pl-4">{children}</ul>,
  ol: ({ children }) => <ol className="my-1 list-decimal pl-4">{children}</ol>,
  li: ({ children }) => <li className="my-0.5">{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
  a: ({ children, href }) => (
    <a href={href} className="text-primary hover:underline" target="_blank" rel="noreferrer">
      {children}
    </a>
  ),
  code: ({ children }) => <code className="rounded bg-muted px-1 py-0.5 text-xs">{children}</code>,
};

export function ReleaseNotesView({ notes, renderer, className }: ReleaseNotesViewProps) {
  if (!notes) return null;
  return (
    <div className={cn("max-h-48 overflow-y-auto text-sm text-muted-foreground", className)}>
      {renderer ? renderer(notes) : <Markdown components={components}>{notes}</Markdown>}
    </div>
  );
}
