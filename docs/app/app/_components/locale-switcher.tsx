"use client";

// 语言切换。
//
// 放设置页而不是顶栏：切语言是低频动作（一辈子可能只做一次），常驻入口会占掉窄屏顶栏本就
// 紧张的横向空间，而那里要留给节点状态——那个才是需要一直可见的东西。
//
// 切换后**立刻持久化**，下次访问优先于浏览器偏好；不改 URL，也不重载页面——
// Lingui 的 `I18nProvider` 会在目录激活后重渲染整棵子树。

import { Trans, useLingui } from "@lingui/react/macro";
import { Languages } from "lucide-react";
import { cn } from "@/lib/cn";
import { LOCALES, type Locale } from "../_lib/i18n";
import { useLocaleSwitcher } from "./i18n-provider";

/** 语言的**自称**（endonym）：给英语用户看 "English" 而不是「英语」，才认得出来。 */
const LOCALE_ENDONYM: Record<Locale, string> = {
  zh: "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
};

export function LocaleSwitcher() {
  const { t } = useLingui();
  const { locale, switchTo } = useLocaleSwitcher();

  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-xs sm:p-6">
      <div className="flex items-center gap-2">
        <Languages className="size-4 text-muted-foreground" aria-hidden />
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>语言</Trans>
        </h2>
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
                "min-h-11 rounded-full border px-4 text-xs transition-colors sm:min-h-9",
                active
                  ? "border-transparent bg-[var(--brand-solid)] text-[var(--brand-ink)]"
                  : "text-muted-foreground hover:bg-accent",
              )}
            >
              {LOCALE_ENDONYM[candidate]}
            </button>
          );
        })}
      </div>
    </section>
  );
}
