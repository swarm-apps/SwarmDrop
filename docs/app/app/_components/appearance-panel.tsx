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
// 桌面 `settings/index.lazy.tsx` 的 `AppearanceSection` 就是「主题 + 语言」，连控件形态
// 都对齐：主题是三张固定预览缩略图、语言是一个 `Select`。它们是同一类东西（跟内容无关的
// 呈现偏好），分成两块会让本就只有四块的设置页更碎。
//
// ## 主题预览缩略图**固定展示该主题的样子**，不跟随当前主题
//
// 与桌面同款。一个跟着当前主题变色的预览图什么也没预览到——用户要看的正是「切过去会长
// 什么样」。

import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { Palette } from "lucide-react";
import { useTheme } from "next-themes";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/cn";
import { LOCALE_ENDONYM, LOCALES, type Locale } from "../_lib/i18n";
import { useMounted } from "../_lib/use-mounted";
import { useLocaleSwitcher } from "./i18n-provider";
import { SettingsCard, SettingsRow, SettingsSection } from "./settings-primitives";

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

  // 挂载后才显示选中态——这一帧的代价是三个按钮短暂都不高亮。
  // 为什么必须这样（静态导出 + localStorage 主题 = hydration mismatch）见 `useMounted`。
  const mounted = useMounted();

  return (
    <SettingsSection icon={Palette} title={<Trans>外观</Trans>}>
      <SettingsCard>
        {/* 主题走整行：三张缩略图是**看**的，塞进右侧控件位会小到看不清预览的是什么。 */}
        <SettingsRow
          title={<Trans>主题</Trans>}
          description={<Trans>选择应用的外观主题</Trans>}
        >
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
        </SettingsRow>

        {/*
          语言改用 `Select`（此前是三枚胶囊按钮），与桌面 `AppearanceSection` 一致。
          胶囊把三个选项全摊开，在只有两行的设置卡里比标题本身还占地方；下拉是设置页的
          常规控件形态，也让这张卡矮到能和「关于」在同一行齐平。

          选项文案仍是**自称**（endonym）：给英语用户看 "English" 而不是「英语」，
          否则切错了就找不回来。
        */}
        <SettingsRow
          title={<Trans>语言</Trans>}
          description={<Trans>选择后会记住；未选择时跟随浏览器的语言偏好</Trans>}
          action={
            <Select value={locale} onValueChange={(value) => void switchTo(value as Locale)}>
              <SelectTrigger className="w-full min-h-11 sm:min-h-9 sm:w-35" aria-label={t`界面语言`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {LOCALES.map((candidate) => (
                  <SelectItem key={candidate} value={candidate}>
                    {LOCALE_ENDONYM[candidate]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
      </SettingsCard>
    </SettingsSection>
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
