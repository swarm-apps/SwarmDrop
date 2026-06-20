import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface ReleaseNotesViewProps {
  notes?: string;
  /** 自定义渲染(如接 Markdown 渲染器);缺省按纯文本渲染(保留换行)。 */
  renderer?: (notes: string) => ReactNode;
  className?: string;
}

export function ReleaseNotesView({ notes, renderer, className }: ReleaseNotesViewProps) {
  if (!notes) return null;
  return (
    <div
      className={cn(
        "max-h-48 overflow-y-auto whitespace-pre-wrap text-sm text-muted-foreground",
        className,
      )}
    >
      {renderer ? renderer(notes) : notes}
    </div>
  );
}
