import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/utils";

type SettingsSectionProps = {
  title: ReactNode;
  icon?: ComponentType<{ className?: string }>;
  children: ReactNode;
  className?: string;
};

export function SettingsSection({
  title,
  icon: Icon,
  children,
  className,
}: SettingsSectionProps) {
  return (
    <section className={cn("flex flex-col gap-3", className)}>
      <SettingsSectionHeader title={title} icon={Icon} />
      {children}
    </section>
  );
}

type SettingsSectionHeaderProps = {
  title: ReactNode;
  icon?: ComponentType<{ className?: string }>;
};

export function SettingsSectionHeader({
  title,
  icon: Icon,
}: SettingsSectionHeaderProps) {
  return (
    <div className="flex items-center gap-2">
      {Icon ? (
        <Icon className="size-4 text-blue-600 dark:text-blue-300" />
      ) : null}
      <h2 className="text-sm font-semibold text-foreground">{title}</h2>
    </div>
  );
}

type SettingsCardProps = {
  children: ReactNode;
  className?: string;
};

export function SettingsCard({ children, className }: SettingsCardProps) {
  return (
    <div className={cn("glass-card rounded-lg", className)}>
      {children}
    </div>
  );
}

type SettingsRowProps = {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
  contentClassName?: string;
};

export function SettingsRow({
  title,
  description,
  action,
  className,
  contentClassName,
}: SettingsRowProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-b border-border p-4 last:border-b-0",
        className,
      )}
    >
      <div className={cn("flex min-w-0 flex-col gap-0.5", contentClassName)}>
        <div className="text-sm font-medium text-foreground">{title}</div>
        {description ? (
          <span className="text-xs text-muted-foreground">{description}</span>
        ) : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}
