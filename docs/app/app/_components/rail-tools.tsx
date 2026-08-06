"use client";

// 侧栏底部的环境入口：主题 / 语言 / 使用文档，收在一枚菜单按钮之后。
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
// ## 为什么是一枚菜单入口，而不是三枚并排的图标（2026-08-06）
//
// 此前是三枚 ghost 图标独占一行、与状态 pill 上下分列。想把两者并进同一行、让底部读成
// 一条底栏时撞了墙：**放不下**。实测 lg 档（侧栏 224px，容器内 203px 可用）——
//
//   运行中           70 + 8 + 116 = 194   ✓
//   running          78 + 8 + 116 = 202   ✓（余 1px）
//   not started      98 + 8 + 116 = 222   ✗ 溢出 19
//   failed to start 111 + 8 + 116 = 235   ✗ 溢出 32
//
// 一行只在「运行中 / running」这两个态里成立，而**恰恰是「未启动」和「启动失败」这两个
// 真正需要读那行字的时刻会被截断**——运行正常时那颗绿点已经说完了，没人去读那三个字。
// 缩小按钮救不回来：36→28px 仍差 8px，还赔上触达尺寸。
//
// 收成一枚入口后 111 + 8 + 36 = 155，所有语言的所有状态都放得下，底部块从 101px 降到 ~60px。
//
// 代价是语言与文档各多一跳。**主题不受影响**——菜单里直接列三项，仍是两次点击，
// 下面那条「切主题是高频、低承诺的动作」的判断没变；被推远的是两个低频入口。
// 语言用子菜单而不是平铺，是为了路线图上那 5 种待加的 locale：3 种平铺尚可，8 种会把
// 这个菜单撑成一列滚动条。
//
// ## 触发图标为什么不是齿轮
//
// 侧栏上面第三项「设置」就是齿轮。同一条侧栏里摆两个齿轮，用户没有办法从图标上分辨
// 哪个是路由、哪个是菜单。`SlidersHorizontal`（调节滑块）正好对应上面那句「调一下环境」，
// 且不与任何导航项撞脸。它也**不再反映当前主题**（此前触发图标画的是当前主题那枚）——
// 当前是浅是深，整个页面自己就在回答，图标复述一遍是冗余；菜单里的对勾才是需要它的地方。
//
// ## 主题与语言在设置页已经有了，为什么这里还要有
//
// 那边是**完整形态**（三张主题缩略图 + 语言 Select），这里是**快捷方式**。两者不冲突：
// 切主题是个高频、低承诺的动作，为它跳去设置页再跳回来，代价比动作本身大。
// 而设置页那份仍然是唯一说明「有哪些选项、分别长什么样」的地方——缩略图在这个尺寸下
// 塞不进来，也不该塞。
//
// ## 「使用文档」为什么不是一整行
//
// 它此前独占一行、图标加文字，视觉重量与上面三条主路由持平——而它是**离开应用**的链接
// （并且离开 `/app` 会卸载节点单例、中断正在进行的传输）。一个指向站外的入口不该和
// 「设备」「收件箱」看起来一样重。降级之后它仍然在场——知识库那条「从 chrome 上拿掉的
// 东西必须重新出现」说的是**拿掉**，这里是降级。
//
// ## 三档断点下的形态
//
// 展开侧栏（≥1024）与状态 pill 同一行、贴右端；图标侧栏（768–1023）竖排在 pill 下方。
// 窄屏没有侧栏，这条入口随之不渲染——主题与语言在那一档仍可从设置页进（底部导航三项之一），
// **但文档没有**：窄屏本来就没有文档入口，这是本次改动之前就存在的缺口，不是它引入的。

import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { ArrowUpRight, BookText, Check, Languages, Monitor, Moon, SlidersHorizontal, Sun } from "lucide-react";
import Link from "next/link";
import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/cn";
import { LOCALE_ENDONYM, LOCALES, localeEndonym } from "../_lib/i18n";
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
 * 触发按钮走 `Button variant="ghost" size="icon"`，不手写尺寸——尺寸规则属于按钮而不属于
 * 调用点（`components/ui/button.tsx` 的头注释明写「从桌面同步这个文件时不要覆盖掉这段」）。
 *
 * ⚠️ **但这里实际恒为 36px，`size="icon"` 的 44px 那一档在本组件下永远不命中。**
 * `size="icon"` 是 `size-11 sm:size-9`，而 `sm:` 是 640px；本组件只在 `md:flex` 的侧栏里
 * 渲染（≥768px），也就是说它可见的每一个视口都已经越过 `sm:`。
 *
 * 这是**布局逼出来的取舍，不是疏忽**：图标侧栏那一档宽 64px（`md:w-16`），减去容器的
 * 左右 padding 只剩 44px，44px 的按钮再算上焦点环塞不进去。要满足触达标准得先加宽侧栏，
 * 而 64px 是 DESIGN.md 定死的三档形态之一。
 *
 * 后果记在这里，别让下一次 a11y 审计以为这块已经过关：768–1023px 的触屏（iPad 竖屏等）
 * 上这枚是 36×36。真要修，入口是侧栏宽度而不是这个常量。
 * （收成一枚之后至少不再有「竖排三枚、彼此只隔 4px」的连带问题。）
 */
const TRIGGER_CLASS = "rounded-lg text-muted-foreground";

export function RailTools() {
  const { t } = useLingui();
  const { locale, switchTo } = useLocaleSwitcher();
  const { theme, setTheme } = useTheme();

  // 首帧一律按「跟随系统」判定对勾：真实主题存在 localStorage，静态导出的 HTML 读不到它。
  // 理由与代价见 `useMounted`。
  const current = useMounted() ? theme : "system";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t`外观与语言`}
          title={t`外观与语言`}
          className={TRIGGER_CLASS}
        >
          <SlidersHorizontal className="size-4" aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>

      {/* `align="end"` 让菜单右边缘对齐按钮：lg 档按钮贴在侧栏右端，菜单于是正好盖住侧栏
          而不是探进内容区。图标档按钮居中在 64px 里，Radix 的碰撞检测会把它推回可视区。 */}
      <DropdownMenuContent align="end" side="top" className="w-48">
        <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">
          {t`主题`}
        </DropdownMenuLabel>
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

        <DropdownMenuSeparator />

        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="gap-2">
            <Languages className="size-4" aria-hidden="true" />
            <span className="flex-1">{t`语言`}</span>
            {/* 当前语言写在触发行上：不展开子菜单就知道现在是哪种，把「多一跳」的代价
                还回去一半。自称形式的理由见 `LOCALE_ENDONYM`。 */}
            <span className="text-xs text-muted-foreground">{localeEndonym(locale)}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {/* 顺序取 `LOCALES` 而不是 `Object.keys`：那份常量是 locale 清单的事实源，
                加一种语言时这里自动跟上。 */}
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
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        <DropdownMenuSeparator />

        {/* 站外链接：右侧的外链箭头是它与上面几项的唯一区别——点它会离开应用区，
            而离开 `/app` 会卸载节点单例、中断正在进行的传输。 */}
        <DropdownMenuItem asChild className="gap-2">
          <Link href="/docs">
            <BookText className="size-4" aria-hidden="true" />
            <span className="flex-1">{t`使用文档`}</span>
            <ArrowUpRight className="size-3.5 text-muted-foreground" aria-hidden="true" />
          </Link>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
