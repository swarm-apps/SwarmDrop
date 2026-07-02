import { ArrowLeft } from "lucide-react";
import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

type IconComp = ComponentType<{ className?: string }>;

export function TaskPageShell({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <main className={cn("flex h-full flex-1 flex-col bg-transparent", className)}>
      {children}
    </main>
  );
}

export function TaskToolbar({
  title,
  onBack,
  trailing,
}: {
  title: ReactNode;
  onBack: () => void;
  trailing?: ReactNode;
}) {
  return (
    <header className="flex h-13 shrink-0 items-center justify-between gap-3 border-b border-white/35 bg-white/[0.16] px-4 shadow-[0_1px_0_rgba(255,255,255,0.28)] backdrop-blur-xl dark:border-white/10 dark:bg-slate-950/[0.10] lg:px-5">
      <button
        type="button"
        onClick={onBack}
        className="group flex min-w-0 items-center gap-2 rounded-full px-1.5 py-1 pr-3 text-[15px] font-medium text-foreground transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-foreground/[0.045] active:scale-[0.98] dark:hover:bg-white/[0.07]"
      >
        <span className="glass-control flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:-translate-x-0.5 group-hover:text-foreground">
          <ArrowLeft className="size-3.5" />
        </span>
        <span className="truncate">{title}</span>
      </button>
      {trailing ? <div className="shrink-0">{trailing}</div> : null}
    </header>
  );
}

export function TaskContent({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className={cn("mx-auto h-full w-full max-w-[1180px] p-5 lg:p-7", className)}>
        {children}
      </div>
    </div>
  );
}

export function GlassPanel({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cn(
        "glass-panel rounded-[28px] p-2 shadow-[0_22px_70px_rgba(15,23,42,0.07)] dark:shadow-[0_24px_82px_rgba(0,0,0,0.24)]",
        className,
      )}
    >
      <div className="h-full rounded-[22px] bg-white/38 shadow-[inset_0_1px_0_rgba(255,255,255,0.48)] dark:bg-white/[0.035] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
        {children}
      </div>
    </section>
  );
}

export function TaskHeroPanel({
  icon: Icon,
  label,
  title,
  description,
  children,
  className,
}: {
  icon?: IconComp;
  label?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <GlassPanel className={className}>
      <div className="flex h-full flex-col gap-5 p-5 lg:p-6">
        <div className="flex min-w-0 items-start gap-3.5">
          {Icon ? (
            <span className="glass-control flex size-11 shrink-0 items-center justify-center rounded-[17px] text-brand">
              <Icon className="size-5" />
            </span>
          ) : null}
          <div className="min-w-0">
            {label ? (
              <p className="text-[12px] font-medium text-brand">
                {label}
              </p>
            ) : null}
            <h1 className="mt-1 text-xl font-semibold tracking-tight text-foreground lg:text-2xl">
              {title}
            </h1>
            {description ? (
              <p className="mt-2 max-w-[54ch] text-sm leading-6 text-muted-foreground">
                {description}
              </p>
            ) : null}
          </div>
        </div>
        {children ? <div className="min-w-0 flex-1">{children}</div> : null}
      </div>
    </GlassPanel>
  );
}

export function CommandDock({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "glass-panel flex shrink-0 items-center justify-end gap-2 rounded-[22px] p-2",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function TaskButton({
  children,
  className,
  variant = "default",
  ...props
}: React.ComponentProps<typeof Button>) {
  return (
    <Button
      variant={variant}
      className={cn(
        "rounded-full px-5 transition-[background-color,color,transform,box-shadow] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.98]",
        variant === "default" &&
          "bg-primary text-primary-foreground shadow-[0_10px_22px_rgba(219,163,65,0.18)] hover:bg-primary/90",
        variant === "outline" &&
          "glass-control border-transparent bg-transparent shadow-none hover:bg-white/50 dark:hover:bg-white/[0.06]",
        variant === "secondary" &&
          "glass-control border-transparent bg-transparent shadow-none hover:bg-white/50 dark:hover:bg-white/[0.06]",
        className,
      )}
      {...props}
    >
      {children}
    </Button>
  );
}

export function InfoTile({
  label,
  value,
  icon: Icon,
  className,
}: {
  label: ReactNode;
  value: ReactNode;
  icon?: IconComp;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "glass-control rounded-[18px] px-3.5 py-3 text-left",
        className,
      )}
    >
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        {Icon ? <Icon className="size-3.5" /> : null}
        {label}
      </div>
      <div className="mt-1.5 min-w-0 text-sm font-semibold text-foreground">
        {value}
      </div>
    </div>
  );
}
