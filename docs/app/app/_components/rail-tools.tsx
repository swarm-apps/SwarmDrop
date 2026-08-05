"use client";

// 侧栏底部的工具条：主题 / 语言 / 使用文档。
//
// ## 为什么这三个凑在一起
//
// 它们是**跟内容无关的应用级开关**——不属于任何一条路由，改了也不影响正在做的事。
// 设备、收件箱、设置那三项导航是「去某个地方」，这三个是「调一下环境」，
// 两类东西不该混在同一列表里。分隔线上方是去处，下方是调节。
//
// 它们因此**不进 `_lib/nav.ts`**：那份事实源的单位是**路由**（每项都有 href、喂构建期
// metadata、参与 active 高亮与底部导航）。这三枚没有 href，塞进去要先编一个，再在
// `APP_NAV` 的 filter、`BY_HREF`、`activeNavHref` 三处开例外——那才是把事实源退化成注释。
//
// ## 主题与语言在设置页已经有了，为什么这里还要有
//
// 那边是**完整形态**（三张主题缩略图 + 语言 Select），这里是**快捷方式**。两者不冲突：
// 切主题是个高频、低承诺的动作，为它跳去设置页再跳回来，代价比动作本身大。
// 而设置页那份仍然是唯一说明「有哪些选项、分别长什么样」的地方——缩略图在这个尺寸下
// 塞不进来，也不该塞。
//
// ## 「使用文档」为什么从一整行变成一枚图标
//
// 它此前独占一行、图标加文字，视觉重量与上面三条主路由持平——而它是**离开应用**的链接
// （并且离开 `/app` 会卸载节点单例、中断正在进行的传输）。一个指向站外的入口不该和
// 「设备」「收件箱」看起来一样重。降成图标之后它仍然在场——知识库那条「从 chrome 上
// 拿掉的东西必须重新出现」说的是**拿掉**，这里是降级。
//
// ## 三档断点下的形态
//
// 展开侧栏（≥1024）一行三枚；图标侧栏（768–1023）竖排三枚。窄屏没有侧栏，这条工具条随之
// 不渲染——主题与语言在那一档仍可从设置页进（底部导航三项之一），**但文档没有**：
// 窄屏本来就没有文档入口，这是本次改动之前就存在的缺口，不是它引入的。

import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { BookText, Check, Languages, Monitor, Moon, Sun } from "lucide-react";
import Link from "next/link";
import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/cn";
import { LOCALE_ENDONYM, LOCALES } from "../_lib/i18n";
import { useMounted } from "../_lib/use-mounted";
import { useLocaleSwitcher } from "./i18n-provider";

/**
 * 主题的取值、图标与标签。
 *
 * **刻意不与设置页 `appearance-panel.tsx` 的那份共用**：那边是三张固定预览缩略图，这边是
 * 一列带对勾的菜单项，形态本质不同，能共享的只有三个字符串——而它们是同一批 msgid，
 * Lingui 的目录里本来就只存一条，重复的是代码不是翻译。
 * 主题取值是**封闭全集**（system / light / dark），不存在「加一种要同改两处」的风险。
 */
const THEMES = [
  { value: "system", icon: Monitor, label: msg`跟随系统` },
  { value: "light", icon: Sun, label: msg`浅色` },
  { value: "dark", icon: Moon, label: msg`深色` },
];

/**
 * 三枚都走 `Button variant="ghost" size="icon"`，不手写尺寸。
 *
 * `size="icon"` 是 `size-11 sm:size-9`——移动端 44px 触达、桌面 36px，那条规则属于按钮
 * 而不属于调用点（`components/ui/button.tsx` 的头注释明写「从桌面同步这个文件时不要覆盖
 * 掉这段」）。此前这里手写 `size-9`，在触屏上就只有 36px，而注释还声称是 44px。
 *
 * 只覆写与侧栏其余控件对齐所需的两项：圆角随导航项的 `rounded-lg`，静息色随其余次要控件
 * 的 `text-muted-foreground`。
 */
const TOOL_CLASS = "rounded-lg text-muted-foreground";

export function RailTools() {
  const { t } = useLingui();
  const { locale, switchTo } = useLocaleSwitcher();
  const { theme, setTheme } = useTheme();

  // 首帧一律画「跟随系统」那枚图标：真实主题存在 localStorage，静态导出的 HTML 读不到它。
  // 理由与代价见 `useMounted`。
  const current = useMounted() ? theme : "system";
  const ThemeIcon = THEMES.find((item) => item.value === current)?.icon ?? Monitor;

  return (
    <div className="flex items-center gap-1 md:flex-col lg:flex-row">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" aria-label={t`主题`} title={t`主题`} className={TOOL_CLASS}>
            <ThemeIcon className="size-4" aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="top">
          {THEMES.map((item) => (
            <DropdownMenuItem key={item.value} onSelect={() => setTheme(item.value)} className="gap-2">
              <item.icon className="size-4" aria-hidden="true" />
              <span className="flex-1">{t(item.label)}</span>
              {/* 选中态用对勾而不是高亮整行：这一列每项都带图标，再给一行上底色会让
                  「当前是哪个」和「鼠标停在哪个」两种反馈撞在一起。 */}
              <Check
                className={cn("size-3.5", current === item.value ? "opacity-100" : "opacity-0")}
                aria-hidden="true"
              />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" aria-label={t`语言`} title={t`语言`} className={TOOL_CLASS}>
            <Languages className="size-4" aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="top">
          {/* 顺序取 `LOCALES` 而不是 `Object.keys`：那份常量是 locale 清单的事实源，
              加一种语言时这里自动跟上。名字用自称，理由见 `LOCALE_ENDONYM`。 */}
          {LOCALES.map((value) => (
            <DropdownMenuItem
              key={value}
              // 切换会 await 目录 chunk，失败时 `switchTo` 自己不写偏好（见 i18n-provider）。
              // 这里不接错误：菜单项已经关掉了，弹一个 toast 说「语言没切成」帮不上忙，
              // 而界面语言没变本身就是最直接的反馈。
              onSelect={() => void switchTo(value)}
              className="gap-2"
            >
              <span className="flex-1">{LOCALE_ENDONYM[value]}</span>
              <Check
                className={cn("size-3.5", locale === value ? "opacity-100" : "opacity-0")}
                aria-hidden="true"
              />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <Button variant="ghost" size="icon" asChild className={TOOL_CLASS}>
        <Link href="/docs" aria-label={t`使用文档`} title={t`使用文档`}>
          <BookText className="size-4" aria-hidden="true" />
        </Link>
      </Button>
    </div>
  );
}
