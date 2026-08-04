"use client";

// 关于：产品是什么 + 去哪儿看文档 / 源码 / 更新日志。
//
// Web 应用区此前没有「关于」区——三端里唯一的缺口。桌面有一整块（品牌卡 + 版本 + 官网 /
// GitHub / 更新日志 + 检查更新）；这里**刻意不做版本号与检查更新**：
//
//   · 版本号：Web 端没有「已安装的版本」这个概念，页面永远是刚从服务器取回的最新一份。
//     摆一个构建号只对排查问题有用，而那属于诊断不属于设置。
//   · 检查更新：更新在刷新页面时自动发生，给一个按钮等于让用户管一件他管不着的事。
//
// 三端「关于」的共同部分是**去处**（文档 / 源码 / 更新日志），差异部分是各自的分发方式。

import { Trans } from "@lingui/react/macro";
import { BookText, ExternalLink, Github, Info, ScrollText } from "lucide-react";
import Link from "next/link";
import { appName, appTagline } from "@/lib/shared";
import { SectionHeader, SectionShell } from "./section";

const REPO = "https://github.com/swarm-apps/SwarmDrop";

export function AboutPanel() {
  return (
    <SectionShell>
      <SectionHeader icon={Info} title={<Trans>关于</Trans>} />

      <div className="flex items-start gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/12 text-brand">
          <ScrollText className="size-5" aria-hidden />
        </div>
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">{appName}</p>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{appTagline}</p>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            <Trans>
              去中心化、端到端加密的设备间传输。无账号、无服务器，文件不经过任何第三方。
            </Trans>
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-1 border-t pt-3">
        {/* 站内链接走 next/link（子路径部署下手写 <a href> 会 404）；站外链接才用 <a>。 */}
        <Link
          href="/docs"
          className="focus-ring flex min-h-11 items-center gap-2.5 rounded-lg px-2 text-sm text-foreground transition-colors hover:bg-accent"
        >
          <BookText className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <Trans>使用文档</Trans>
        </Link>
        <ExternalRow href={REPO} icon={Github}>
          <Trans>源码仓库</Trans>
        </ExternalRow>
        <ExternalRow href={`${REPO}/blob/main/CHANGELOG.md`} icon={ScrollText}>
          <Trans>更新日志</Trans>
        </ExternalRow>
      </div>
    </SectionShell>
  );
}

function ExternalRow({
  href,
  icon: Icon,
  children,
}: {
  href: string;
  icon: typeof Github;
  children: React.ReactNode;
}) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="focus-ring flex min-h-11 items-center gap-2.5 rounded-lg px-2 text-sm text-foreground transition-colors hover:bg-accent"
    >
      <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="flex-1">{children}</span>
      {/* 新标签页打开是外链的默认行为，图标是它唯一的预告——尤其在应用区，
          用户有理由担心「点了会不会离开、正在跑的传输会不会断」。 */}
      <ExternalLink className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
    </a>
  );
}
