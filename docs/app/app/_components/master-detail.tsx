"use client";

// 主从（master-detail）响应式外壳。收件箱与传输页共用同一套标准：
//
//   ≥920px  左列表 + 右详情双栏
//   <920px  详情占满，列表从左侧抽屉滑出（Esc / 点遮罩关闭，面板 inert + 焦点移入）
//
// 断点、抽屉方向、抽屉行为在这里定义一次；两页只提供 list / detail 内容，
// 保证「什么时候进断点、往哪边开」全应用一个标准，而非各页手抄。
//
// ## 与桌面 `src/components/layout/master-detail-shell.tsx` 的关系
//
// **同一套交互标准、同一个断点，两份实现。** 不共享组件是刻意的：桌面那份用玻璃拟态
// （`glass-panel` / `--glass-control-border`）和为鼠标调的尺寸，搬进 Web 要把整套玻璃 token
// 一起搬过来；而 Web 的基线视口是手机。共享的是 `MASTER_DETAIL_QUERY` 这个数与这段行为描述，
// 见 `_lib/use-media-query.ts` 的注释——那里与桌面的同名常量互指。

import { PanelLeft } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { useIsWideLayout } from "../_lib/use-media-query";

/** 从左侧滑出的内容抽屉。窄屏下列表住在这里。 */
function SlideDrawer({
  open,
  onClose,
  label,
  children,
}: {
  open: boolean;
  onClose: () => void;
  label: string;
  children: ReactNode;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  // onClose 的调用点都是内联箭头，随父级重渲染换引用；用 ref 读最新值，让 effect 只依赖 open。
  // 否则抽屉打开期间父级每渲染一次就解绑重绑监听 + 强制 focus——收件箱的搜索框就在抽屉里，
  // 等于每敲一个字空转一轮，还会把焦点从输入框抢回面板。
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    document.addEventListener("keydown", onKey);
    const raf = requestAnimationFrame(() => panelRef.current?.focus());
    return () => {
      document.removeEventListener("keydown", onKey);
      cancelAnimationFrame(raf);
    };
  }, [open]);

  return (
    <div className={cn("fixed inset-0 z-30", !open && "pointer-events-none")}>
      {/*
        遮罩是纯装饰 + 一个点击目标，故 `aria-hidden`：它不该出现在读屏的可聚焦序列里
        （早先写成 `<button aria-label={label}>` 是错的——那会让读屏念出一个顶着抽屉名字的
        全屏按钮）。键盘关闭走 Esc，见上面的 effect；面板本身是 `role="dialog" aria-modal`。
      */}
      <div
        aria-hidden="true"
        onClick={onClose}
        className={cn(
          "absolute inset-0 bg-black/40 transition-opacity duration-300 motion-reduce:transition-none",
          open ? "opacity-100" : "opacity-0",
        )}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        tabIndex={-1}
        inert={!open}
        className={cn(
          "absolute inset-y-0 left-0 flex w-[86%] max-w-[344px] flex-col overflow-y-auto rounded-r-2xl border-r bg-background shadow-2xl outline-hidden transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none",
          open ? "translate-x-0" : "-translate-x-full",
        )}
      >
        {children}
      </div>
    </div>
  );
}

interface ListContext {
  /** 选中一项后关闭抽屉（窄屏）；宽屏为无害 no-op。 */
  closeDrawer: () => void;
}

interface DetailContext {
  /** 窄屏下打开列表抽屉；宽屏为 null（列表常驻左栏，无需按钮）。 */
  openList: (() => void) | null;
}

export function MasterDetail({
  list,
  detail,
  drawerLabel,
  testId,
}: {
  list: (ctx: ListContext) => ReactNode;
  detail: (ctx: DetailContext) => ReactNode;
  drawerLabel: string;
  testId?: string;
}) {
  const isWide = useIsWideLayout();
  const [drawerOpen, setDrawerOpen] = useState(false);

  // 稳定引用：`list()` 会把它透传给列表行，每帧新建的闭包会打穿下游的 memo。
  const closeDrawer = useCallback(() => setDrawerOpen(false), []);

  // 切到宽屏时收起抽屉，避免状态残留（回到窄屏时抽屉又莫名开着）。
  useEffect(() => {
    if (isWide) setDrawerOpen(false);
  }, [isWide]);

  if (isWide) {
    return (
      <div
        data-testid={testId}
        className="grid gap-4"
        style={{ gridTemplateColumns: "minmax(260px, 340px) minmax(0, 1fr)" }}
      >
        <section className="flex min-h-0 flex-col overflow-hidden rounded-xl border bg-card shadow-xs">
          {list({ closeDrawer })}
        </section>
        {detail({ openList: null })}
      </div>
    );
  }

  return (
    <div data-testid={testId} className="flex flex-col">
      {detail({ openList: () => setDrawerOpen(true) })}
      <SlideDrawer open={drawerOpen} onClose={closeDrawer} label={drawerLabel}>
        {list({ closeDrawer })}
      </SlideDrawer>
    </div>
  );
}

/**
 * 详情头部的「打开列表」按钮。窄屏详情复用同一坐标同一样式的唤出入口；
 * `openList` 为 null（宽屏）时不渲染。
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
    <button
      type="button"
      onClick={openList}
      aria-label={label}
      title={label}
      className="-m-1.5 flex size-11 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      <PanelLeft className="size-4" aria-hidden />
    </button>
  );
}
