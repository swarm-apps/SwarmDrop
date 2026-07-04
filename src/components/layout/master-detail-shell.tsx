/**
 * MasterDetailShell
 * 主从（master-detail）响应式外壳，收件箱与活动中心共用同一套标准：
 * - 宽屏（≥920px，见 MASTER_DETAIL_QUERY）：左列表 + 右详情双栏。
 * - 窄屏（<920px）：详情占满，列表从左侧抽屉滑出（遮罩只暗内容、不盖全局顶栏，
 *   Esc / 点遮罩关闭，面板 inert + 焦点移入）。
 *
 * 断点、抽屉方向、抽屉行为全在这里定义一次；两页只提供 list / detail 内容，
 * 保证「什么时候进断点、往哪边开」全局一个标准，而非各页手抄。
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { PanelLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useIsWideLayout } from "@/hooks/use-media-query";

interface DetailContext {
  /** 窄屏下打开列表抽屉；宽屏为 null（列表常驻左栏，无需按钮）。 */
  openList: (() => void) | null;
  /** 是否窄屏态（详情组件据此决定内部滚动/容器行为）。 */
  isCompact: boolean;
}

interface ListContext {
  /** 选中一项后关闭抽屉（窄屏）；宽屏为 no-op。 */
  closeDrawer: () => void;
}

export function MasterDetailShell({
  list,
  detail,
  drawerLabel,
  /** 宽屏左栏最大宽度（px）。 */
  listMaxWidth = 360,
}: {
  list: (ctx: ListContext) => ReactNode;
  detail: (ctx: DetailContext) => ReactNode;
  drawerLabel: string;
  listMaxWidth?: number;
}) {
  const isWide = useIsWideLayout();
  const [drawerOpen, setDrawerOpen] = useState(false);

  // 切到宽屏时收起抽屉，避免状态残留
  useEffect(() => {
    if (isWide) setDrawerOpen(false);
  }, [isWide]);

  // 窄屏抽屉：Esc 关闭 + 焦点移入面板
  const drawerPanelRef = useRef<HTMLDivElement>(null);
  const open = !isWide && drawerOpen;
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDrawerOpen(false);
    };
    document.addEventListener("keydown", onKey);
    const raf = requestAnimationFrame(() => drawerPanelRef.current?.focus());
    return () => {
      document.removeEventListener("keydown", onKey);
      cancelAnimationFrame(raf);
    };
  }, [open]);

  if (isWide) {
    return (
      <main className="relative flex h-full flex-1 flex-col bg-transparent">
        <div
          className="mx-auto grid h-full w-full max-w-[1240px] grid-rows-1 gap-6 overflow-hidden p-6"
          style={{
            gridTemplateColumns: `minmax(300px, ${listMaxWidth}px) minmax(0, 1fr)`,
          }}
        >
          <section className="glass-panel flex min-h-0 flex-col overflow-hidden rounded-[24px]">
            {list({ closeDrawer: () => {} })}
          </section>
          {detail({ openList: null, isCompact: false })}
        </div>
      </main>
    );
  }

  return (
    <main className="relative flex h-full flex-1 flex-col bg-transparent">
      <div className="mx-auto flex h-full w-full max-w-[880px] flex-col overflow-y-auto p-4 sm:p-5">
        {detail({ openList: () => setDrawerOpen(true), isCompact: true })}
      </div>

      {/* 列表抽屉：限定在顶栏下方的内容区滑出，遮罩只暗内容、不盖全局顶栏 */}
      <div className={cn("absolute inset-0 z-30", !drawerOpen && "pointer-events-none")}>
        <div
          onClick={() => setDrawerOpen(false)}
          className={cn(
            "absolute inset-0 bg-black/40 transition-opacity duration-300 motion-reduce:transition-none",
            drawerOpen ? "opacity-100" : "opacity-0",
          )}
        />
        <div
          ref={drawerPanelRef}
          role="dialog"
          aria-modal="true"
          aria-label={drawerLabel}
          tabIndex={-1}
          inert={!drawerOpen}
          className={cn(
            "absolute inset-y-0 left-0 flex w-[86%] max-w-[344px] flex-col rounded-r-[24px] border-r border-[color:var(--glass-control-border)] bg-background shadow-2xl outline-hidden transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none",
            drawerOpen ? "translate-x-0" : "-translate-x-full",
          )}
        >
          {list({ closeDrawer: () => setDrawerOpen(false) })}
        </div>
      </div>
    </main>
  );
}

/**
 * 详情头部前导「打开列表」按钮：窄屏详情复用同一坐标同一样式的唤出按钮。
 * 传入 openList 为 null（宽屏）时不渲染。
 */
export function OpenListButton({
  openList,
  label,
}: {
  openList: (() => void) | null;
  label: string;
}) {
  if (!openList) return null;
  return (
    <Button
      variant="ghost"
      size="icon"
      className="-ml-1 size-8 shrink-0 rounded-md text-muted-foreground hover:text-foreground"
      onClick={openList}
      aria-label={label}
      title={label}
    >
      <PanelLeft className="size-4" />
    </Button>
  );
}
