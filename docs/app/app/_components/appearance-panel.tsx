"use client";

// 外观：主题 + 语言。
//
// ## 主题切换此前在 Web 应用区**完全不存在**
//
// 明暗模式一直是生效的（fumadocs 的 `RootProvider` 内含 next-themes），但应用区没有任何
// 切换入口——`app/app/layout.tsx` 不套 `DocsLayout` / `HomeLayout`，而主题切换器由那两个
// 布局提供。用户唯一的出路是侧栏那条「使用文档」链接跳去文档站切了再回来，而**离开 /app
// 会卸载节点单例、中断正在进行的传输**（那条链接自己的注释里就写着这件事）。
//
// 桌面与移动都有主题设置，这是三端里唯一的缺口。
//
// ## 两者并在一块，对齐桌面的「外观」区
//
// 桌面 `settings/index.lazy.tsx` 的 `AppearanceSection` 就是「主题 + 语言」。它们是同一类
// 东西（跟内容无关的呈现偏好），分成两块会让本就只有四块的设置页更碎。
//
// ## 主题预览缩略图**固定展示该主题的样子**，不跟随当前主题
//
// 与桌面同款。一个跟着当前主题变色的预览图什么也没预览到——用户要看的正是「切过去会长
// 什么样」。

import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { Languages, Palette } from "lucide-react";
import { useTheme } from "next-themes";
import { useEffect, useState } from "react";
import { cn } from "@/lib/cn";
import { LOCALES, type Locale } from "../_lib/i18n";
import { useLocaleSwitcher } from "./i18n-provider";
import { SectionHeader, SectionShell } from "./section";

/** 语言的**自称**（endonym）：给英语用户看 "English" 而不是「英语」，才认得出来。 */
const LOCALE_ENDONYM: Record<Locale, string> = {
  zh: "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
};

type ThemeValue = "system" | "light" | "dark";

const THEMES: ThemeValue[] = ["system", "light", "dark"];

/** 标签存**描述符**，由组件 `t(...)` 展开——翻译宏只能在组件里用（知识库 Lingui 三条约束）。 */
const THEME_LABEL = {
  system: msg`跟随系统`,
  light: msg`浅色`,
  dark: msg`深色`,
};

export function AppearancePanel() {
  const { t } = useLingui();
  const { locale, switchTo } = useLocaleSwitcher();
  const { theme, setTheme } = useTheme();

  // next-themes 在服务端/首帧读不到已选主题（它存在 localStorage）。静态导出下预渲染出来的
  // HTML 一定是「未选中任何一项」，直接渲染 `theme` 会 hydration mismatch。挂载后再显示选中
  // 态——这一帧的代价是三个按钮短暂都不高亮，比整棵子树报 mismatch 好。
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  return (
    <SectionShell>
      <SectionHeader icon={Palette} title={<Trans>外观</Trans>} />

      <div className="flex flex-col gap-2">
        <div>
          <p className="text-sm font-medium text-foreground">
            <Trans>主题</Trans>
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            <Trans>选择应用的外观主题。</Trans>
          </p>
        </div>
        <div role="radiogroup" aria-label={t`应用主题`} className="grid grid-cols-3 gap-2">
          {THEMES.map((value) => (
            <ThemeOption
              key={value}
              value={value}
              label={t(THEME_LABEL[value])}
              active={mounted && theme === value}
              onSelect={() => setTheme(value)}
            />
          ))}
        </div>
      </div>

      <div className="flex flex-col gap-2 border-t pt-4">
        <div className="flex items-center gap-2">
          <Languages className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <p className="text-sm font-medium text-foreground">
            <Trans>语言</Trans>
          </p>
        </div>
        <p className="text-xs text-muted-foreground">
          <Trans>选择后会记住；未选择时跟随浏览器的语言偏好。</Trans>
        </p>
        <div role="radiogroup" aria-label={t`界面语言`} className="flex flex-wrap gap-2">
          {LOCALES.map((candidate) => {
            const active = locale === candidate;
            return (
              <button
                key={candidate}
                type="button"
                role="radio"
                aria-checked={active}
                onClick={() => void switchTo(candidate)}
                className={cn(
                  "focus-ring min-h-11 rounded-full border px-4 text-xs transition-colors sm:min-h-9",
                  active
                    ? "border-transparent bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:bg-accent",
                )}
              >
                {LOCALE_ENDONYM[candidate]}
              </button>
            );
          })}
        </div>
      </div>
    </SectionShell>
  );
}

function ThemeOption({
  value,
  label,
  active,
  onSelect,
}: {
  value: ThemeValue;
  label: string;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onSelect}
      className={cn(
        "focus-ring flex flex-col items-center gap-2 rounded-xl border p-2 transition-colors",
        active
          ? "border-[var(--brand-solid)]/60 bg-[var(--brand-solid)]/5"
          : "hover:bg-accent/40",
      )}
    >
      <ThemePreview value={value} />
      <span className={cn("text-xs font-medium", active ? "text-brand" : "text-muted-foreground")}>
        {label}
      </span>
    </button>
  );
}

/**
 * 主题迷你预览。**固定颜色，不用主题 token**——它画的是「那个主题长什么样」，
 * 跟着当前主题变就等于什么也没预览到。
 */
function ThemePreview({ value }: { value: ThemeValue }) {
  if (value === "system") {
    return (
      <div className="flex h-10 w-full overflow-hidden rounded-lg border" aria-hidden>
        <div className="flex-1 space-y-1 bg-white p-1.5">
          <div className="h-1 w-3/4 rounded-full bg-black/20" />
          <div className="h-1 w-1/2 rounded-full bg-black/10" />
        </div>
        <div className="flex-1 space-y-1 bg-zinc-900 p-1.5">
          <div className="h-1 w-3/4 rounded-full bg-white/30" />
          <div className="h-1 w-1/2 rounded-full bg-white/20" />
        </div>
      </div>
    );
  }

  const isDark = value === "dark";
  return (
    <div
      aria-hidden
      className={cn(
        "h-10 w-full overflow-hidden rounded-lg border",
        isDark ? "border-white/10 bg-zinc-900" : "border-black/10 bg-white",
      )}
    >
      <div
        className={cn(
          "flex h-2.5 items-center gap-0.5 border-b px-1",
          isDark ? "border-white/10" : "border-black/5",
        )}
      >
        <span className={cn("size-1 rounded-full", isDark ? "bg-white/25" : "bg-black/15")} />
        <span className={cn("size-1 rounded-full", isDark ? "bg-white/15" : "bg-black/10")} />
      </div>
      <div className="space-y-1 p-1.5">
        <div className={cn("h-1 w-2/3 rounded-full", isDark ? "bg-white/25" : "bg-black/15")} />
        <div className={cn("h-1 w-1/2 rounded-full", isDark ? "bg-white/15" : "bg-black/10")} />
      </div>
    </div>
  );
}
