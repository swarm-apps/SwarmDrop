"use client";

import { i18n } from "@lingui/core";
import { I18nProvider, useLingui } from "@lingui/react";
import { useEffect, type ReactNode } from "react";
import {
  activateLocale,
  rememberLocale,
  resolveLocale,
  SOURCE_LOCALE,
  type Locale,
} from "../_lib/i18n";

/**
 * 应用区的 i18n 边界。挂在 layout（与 `WebNodeBootstrap` 同一层），跨路由不重挂。
 *
 * ## 为什么首帧一定是源 locale
 *
 * 静态导出在**构建期**就把 HTML 预渲染好了，那一刻没有 `navigator`、没有 `localStorage`，
 * 只能用源 locale。客户端若在首帧就换成别的语言，hydration 会与预渲染的 HTML 对不上，
 * React 直接报 mismatch。
 *
 * 所以顺序是固定的：源 locale 在 `_lib/i18n.ts` 模块加载时**同步**激活 → 首帧与预渲染一致 →
 * hydration 完成后本组件的 effect 解析并激活用户的 locale → 重渲染。非中文用户会看到一瞬间的
 * 中文，这是静态导出下客户端 i18n 的固有代价，换取的是不预生成三套路由。
 *
 * **不要用「渲染 null 直到 locale 就绪」绕开**：那会让首屏空白一帧，而且预渲染出来的 HTML
 * 变成空壳——JS 没加载出来时页面什么都没有。
 */
export function AppI18nProvider({ children }: { children: ReactNode }) {
  useEffect(() => {
    const locale = resolveLocale();
    // 源 locale 在模块加载时已经激活过，不必再来一次。
    if (locale !== SOURCE_LOCALE) void activateLocale(locale);
  }, []);

  return <I18nProvider i18n={i18n}>{children}</I18nProvider>;
}

/**
 * locale 切换器的状态与动作，供设置页使用。
 *
 * 与 [`AppI18nProvider`] 的分工：那个只管「进来时用哪个」，这个管「用户主动改」。
 * 改完立刻持久化——下次访问要优先于浏览器偏好。
 *
 * 读的是 `useLingui()` 而不是模块级单例：前者随 `I18nProvider` 的激活事件重渲染，
 * 直接读单例的组件在切换后不会自己更新。
 */
export function useLocaleSwitcher(): {
  locale: string;
  switchTo: (locale: Locale) => Promise<void>;
} {
  const { i18n: activeI18n } = useLingui();
  return {
    locale: activeI18n.locale,
    // **先激活、成功了再记住**：目录是按需 import 的，chunk 拉不下来时 `activateLocale` 会抛。
    // 反过来先记住的话，用户下次访问仍会去加载那个失败的 locale，而且他已经没有「回到原来
    // 那个」的入口了——界面还是旧语言，偏好却已经改掉。
    switchTo: async (locale) => {
      await activateLocale(locale);
      rememberLocale(locale);
    },
  };
}
