"use client";

// 关于：产品是什么 + 去哪儿看文档 / 源码 / 更新日志。
//
// **形态对齐桌面 `settings/-about-section.tsx` 的 `AboutPanel`**：左侧品牌块（图标 + 名称 +
// 标语 + 一句话说明），右侧一排小号外链按钮，底部一行特性标签。此前这里是「品牌块 + 三行
// 竖排链接列表」，三条链接各占 44px 一行，把这张卡撑到比它承载的信息高一倍。
//
// ## 三处刻意不做（与桌面的差异，都是形态差异不是缺口）
//
//   · **版本号 / 检查更新**：Web 端没有「已安装的版本」这个概念，页面永远是刚从服务器取回的
//     最新一份；更新在刷新时自动发生，给一个按钮等于让用户管一件他管不着的事。
//     桌面那块（版本胶囊 + 更新 banner + 下载进度）整体不存在。
//   · **官网入口**：这个应用**就跑在官网里**（`/app` 是文档站的一条路由），再放一个「官网」
//     按钮是指回自己。桌面是独立窗口，那颗按钮在那边才有意义。
//   · **「使用文档」用 next/link**：它是站内路由，手写 `<a href>` 在子路径部署（GitHub Pages
//     的 /SwarmDrop）下不会被加 basePath，整条链接 404。只有真外链才用 `<a>`。

import { Trans } from "@lingui/react/macro";
import {
  Bot,
  BookText,
  ExternalLink,
  Github,
  Info,
  KeyRound,
  RefreshCw,
  ScrollText,
  ShieldCheck,
} from "lucide-react";
import Link from "next/link";
import type { ComponentType, ReactNode } from "react";
import { appIconPath, appName, appTagline } from "@/lib/shared";
import { SettingsCard, SettingsSection } from "./settings-primitives";

const REPO = "https://github.com/swarm-apps/SwarmDrop";

export function AboutPanel() {
  return (
    <SettingsSection icon={Info} title={<Trans>关于</Trans>}>
      <SettingsCard>
        <div className="flex flex-1 flex-col gap-5 p-4 sm:p-5">
          <div className="flex flex-col gap-4 min-[640px]:flex-row min-[640px]:items-start min-[640px]:justify-between">
            <div className="flex gap-3.5">
              {/* 裸字符串路径要手动拼 basePath——`appIconPath` 已经拼好了（见 lib/shared.ts）。 */}
              <img src={appIconPath} alt="" className="size-12 shrink-0 rounded-2xl" />
              <div className="flex min-w-0 flex-col gap-1.5">
                <span className="text-base font-semibold text-foreground">{appName}</span>
                <p className="text-[13px] font-medium text-foreground/90">{appTagline}</p>
                <p className="max-w-md text-xs leading-5 text-muted-foreground">
                  <Trans>
                    去中心化、端到端加密的设备间传输。无账号、无服务器，文件不经过任何第三方。
                  </Trans>
                </p>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 min-[640px]:shrink-0 min-[640px]:justify-end">
              <LinkButton icon={BookText} href="/docs" internal>
                <Trans>使用文档</Trans>
              </LinkButton>
              <LinkButton icon={Github} href={REPO}>
                GitHub
              </LinkButton>
              <LinkButton icon={ScrollText} href={`${REPO}/blob/main/CHANGELOG.md`}>
                <Trans>更新日志</Trans>
              </LinkButton>
            </div>
          </div>

          {/* 特性标签与桌面同一组（去掉「断点续传」之外全部适用于 Web）。它们不是装饰：
              这一屏是用户唯一会读到「这东西凭什么可信」的地方。 */}
          <div className="mt-auto flex flex-wrap gap-1.5">
            <FeatureTag icon={ShieldCheck} label={<Trans>端到端加密</Trans>} />
            <FeatureTag icon={KeyRound} label={<Trans>无账户 · 无服务器</Trans>} />
            <FeatureTag icon={RefreshCw} label={<Trans>断点续传</Trans>} />
            <FeatureTag icon={Bot} label={<Trans>AI 原生 · MCP</Trans>} />
          </div>
        </div>
      </SettingsCard>
    </SettingsSection>
  );
}

/** 核心特性小标签。与桌面 `FeatureTag` 同形。 */
function FeatureTag({
  icon: Icon,
  label,
}: {
  icon: ComponentType<{ className?: string }>;
  label: ReactNode;
}) {
  return (
    <span className="flex items-center gap-1.5 rounded-full border bg-background/50 px-2.5 py-1 text-[11px] font-medium text-muted-foreground dark:bg-white/[0.03]">
      <Icon className="size-3.5 text-brand" />
      {label}
    </span>
  );
}

/**
 * 外链/站内链按钮。桌面那份是 `<button onClick={openUrl}>`（Tauri 要走 opener 插件），
 * 浏览器里就是链接本身——语义、中键打开、右键复制地址都白拿。
 *
 * `min-h-11`：这一排在手机上也要点得中（≥44px），桌面那份没有这个约束。
 */
function LinkButton({
  icon: Icon,
  href,
  internal = false,
  children,
}: {
  icon: ComponentType<{ className?: string }>;
  href: string;
  /** 站内路由走 next/link（basePath），站外用原生 <a> 并标出「会新开标签页」。 */
  internal?: boolean;
  children: ReactNode;
}) {
  const className =
    "focus-ring flex min-h-11 items-center gap-1.5 rounded-lg border bg-background px-3 text-xs font-medium text-foreground transition-colors hover:bg-accent sm:min-h-8";

  if (internal) {
    return (
      <Link href={href} className={className}>
        <Icon className="size-3.5" />
        {children}
      </Link>
    );
  }

  return (
    <a href={href} target="_blank" rel="noreferrer" className={className}>
      <Icon className="size-3.5" />
      {children}
      {/* 新标签页打开是外链的默认行为，图标是它唯一的预告——在应用区尤其要说，
          用户有理由担心「点了会不会离开、正在跑的传输会不会断」。 */}
      <ExternalLink className="size-3 text-muted-foreground" aria-hidden />
    </a>
  );
}
