import { getDownloadCatalog, type DownloadCatalog } from "@swarm-hive/sdk";
import { ArrowRight, Github } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import type { ReactNode } from "react";
import { DownloadPanel } from "@/components/download-panel";
import { MobileDownloadCard } from "@/components/mobile-download-card";
import { appName, assetPath, links, swarmhiveConfig } from "@/lib/shared";
// 落地页 CTA 直接指向应用区落点，绕开 `/app` 那一跳（它只是给手输地址的人兜底的重定向壳）。
import { APP_HOME } from "@/app/app/_lib/nav";
import { Mono, Section, SectionHead, SHELL } from "./_components/home-primitives";

/*
 * ─────────────────────────────────────────────────────────────────────────────
 * 官网首页（2026-08-06 重写）
 *
 * ## 上一版为什么要整个换掉
 *
 * 两个层面的问题，其中第二个更严重。
 *
 * **一、它在宣传不存在的功能。** 上一版写着「每次传输独立生成 256-bit 密钥，
 * XChaCha20-Poly1305 加密」「私钥落 Stronghold 加密保险库」「生物识别解锁
 * TouchID / FaceID / Windows Hello」「跨网络输入 6 位配对码」「设一个安全密码」。
 * 这五条**全部已经不成立**：应用层加密在 wire v2 整块删除、Stronghold 移除、
 * 生物识别 2026-07-27 移除、6 位分享码被 PairInvite 取代、密码解锁流程整体移除。
 * 官网是这些说法唯一还活着的地方。首页最重要的属性不是好看，是**说的话是真的**。
 *
 * **二、它是 AI 落地页模板的标准形态。** 徽章 pill + 大标题 + 三个 CTA + 右侧插画，
 * 接一排 `5 / 0 / 0 / MIT` 四格数字（impeccable 明令禁止的 hero-metric 模板），
 * 再接三个「圆角图标片 + 标题 + 灰描述」的卡片网格区块——同一个版式在一页里用了三次，
 * 四个区块各顶一枚小号全大写 eyebrow。PRODUCT.md 的反面参照第一条就是
 * 「通用 SaaS 后台套路：渐变色 hero、无脑铺满的卡片网格」。
 *
 * ## 这一版的取舍
 *
 * **真图，不画插画。** 上一版整页没有一张产品截图，hero 是手绘的六边形 SVG 蜂群图。
 * 一个文件传输工具的首页从不展示自己长什么样，是最贵的那种「省事」。现在 hero 与
 * 「三端」区块用的都是**现场截的真实界面**（`public/shots/`，见那里的 README）。
 * 仓库里那支 `public/hero/swarmdrop-hero.mp4` **刻意不用**：它是 Remotion 画的
 * 模拟界面（不是真产品），配色还停在旧的深蓝身份，文案里带着一个没转义的 `\n`。
 *
 * **机器值当质感用。** 页面上出现的 multiaddr 全部取自一台真实运行的节点，只截断不编造
 * （The Mono Truth Rule）。这个产品的可信度来自「把真实链路摊开给你看」，
 * 而不是来自形容词——所以 `~2 ms / 10-100 ms / 100-500 ms` 那种没有出处的
 * 精确数字也一并删掉了。
 *
 * **一处深色，不做明暗交替。** 整页跟随主题，只有 hero 恒定是应用自己的深藏蓝画布
 * （`--tile`）。这是刻意的单点重心，不是「深浅区块交替」——后者会让人以为中途换了网站。
 *
 * **八个区块，八种版式。** hero 分栏 / 平台细条 / 三段链路 / 错位截图 / 定义列表 /
 * 代码分栏 / 下载面板 / 品牌色收尾。同一种版式家族在一页里不重复出现；全页零 eyebrow。
 * ─────────────────────────────────────────────────────────────────────────────
 */

async function getInitialCatalog(appSlug: string): Promise<DownloadCatalog | null> {
  try {
    return await getDownloadCatalog({
      baseUrl: swarmhiveConfig.baseUrl,
      appSlug,
      channel: swarmhiveConfig.channel,
      fetchImpl: fetch,
    });
  } catch {
    return null;
  }
}

export default async function HomePage() {
  const [initialCatalog, initialMobileCatalog] = await Promise.all([
    getInitialCatalog(swarmhiveConfig.appSlug),
    getInitialCatalog(swarmhiveConfig.appSlugMobile),
  ]);

  return (
    // `data-swarmdrop-home` 是 `global.css` 里那条次级文字色覆盖的锚点——
    // fumadocs 的亮色 muted 在这页底色上只有 4.35:1，低于 AA。理由与实测值见那里。
    <main data-swarmdrop-home className="flex flex-1 flex-col overflow-hidden">
      <Hero />
      <PlatformStrip />
      <Routing />
      <ThreeHosts />
      <SecurityLedger />
      <Agents />
      <Downloads catalog={initialCatalog} mobileCatalog={initialMobileCatalog} />
      <Closing />
      <StatusNote />
      <Footer />
    </main>
  );
}

/* ─────────────────────────────  HERO  ───────────────────────────── */

/**
 * 全页唯一恒定深色的区块，底色取应用自己的暗色画布 `--tile`。
 *
 * **为什么值得破一次主题一致性**：产品最好看的样子就是它的暗色界面，而 hero 里那张图
 * 正是暗色截图——放在亮色底上要额外画一层框才不突兀，放在同色底上它自己就融进去了。
 * 「一页一次刻意的整块换色」是被允许的手法，「区块之间深浅交替」不是。
 *
 * 文字元素**四件**封顶（标题 / 副文 / 两个 CTA），没有 eyebrow、没有 CTA 下面那行小字、
 * 没有「已被 XX 团队使用」——那些一律下沉到后面的区块。上一版 hero 有三个 CTA，
 * 第三个（「阅读文档」）现在在页脚与顶栏里。
 */
function Hero() {
  return (
    <section className="relative isolate overflow-hidden bg-[var(--tile)] text-white">
      {/* 品牌青绿从右上斜射进来，与应用里那层极光同源同向。低饱和、大半径——
          它是底色的一部分，不是一枚「发光球」。 */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10"
        style={{
          background:
            "radial-gradient(62% 58% at 76% 18%, color-mix(in oklch, var(--brand-solid) 30%, transparent), transparent 72%)",
        }}
      />
      {/* 赤铜是 logo 中心色，整页只在这里出现一次、且只作为环境光的一丝暖度
          （DESIGN.md：Copper Core 不做一般 UI 强调色）。 */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 opacity-40"
        style={{
          background:
            "radial-gradient(46% 46% at 8% 92%, color-mix(in oklch, var(--brand-warm) 22%, transparent), transparent 70%)",
        }}
      />
      {/* 极细颗粒。深底大面积纯色会显出色带，这一层同时压掉色带、给玻璃一点可揉的东西。 */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 opacity-[0.22] mix-blend-overlay"
        style={{
          backgroundImage:
            "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='3'/%3E%3C/filter%3E%3Crect width='160' height='160' filter='url(%23n)' opacity='0.5'/%3E%3C/svg%3E\")",
        }}
      />

      {/* 文案列略宽于图（1.05 : 0.95）。图是向右出血的，右侧本来就有视觉延伸，
          再让它占更大栅格份额会把标题挤成三行——见下面 `clamp` 上限那条注释。 */}
      <div className={`${SHELL} grid items-center gap-x-12 gap-y-14 pt-16 pb-20 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] lg:pt-24 lg:pb-28`}>
        <div className="flex flex-col items-start">
          {/* **上限 3.5rem（56px）是量出来的，不是审美**：1440 视口下文案列宽 554px，
              这句 17 个字在 56px 下正好落成 2 行（17×56 = 952，952/554 = 1.72）。
              之前是 4.25rem（68px），同一列里排成 **3 行**——「hero 标题桌面端不超过 2 行」
              这条不是排版洁癖，三行标题配一张大图会把首屏的重心从「说了什么」挪到「有多大」。
              旁边有大图时字号本来就该收一档。改大这个数之前先在 1440 下数行数。 */}
          <h1
            className="anim-rise text-balance text-[clamp(2.25rem,4.6vw,3.5rem)] font-bold leading-[1.08] tracking-[-0.03em]"
            style={{ "--d": "0s" } as React.CSSProperties}
          >
            传个文件，不必先交给别人的服务器。
          </h1>

          <p
            className="anim-rise mt-7 max-w-[44ch] text-pretty text-[17px] leading-8 text-white/72"
            style={{ "--d": "0.07s" } as React.CSSProperties}
          >
            无账号，无中央服务器。桌面、手机和浏览器都是真正的传输端；配一次对，之后一直有效。
          </p>

          <div
            className="anim-rise mt-10 flex flex-wrap items-center gap-3"
            style={{ "--d": "0.14s" } as React.CSSProperties}
          >
            {/* 主 CTA 指向浏览器端而不是下载：它是**零安装**的完整客户端，
                「不用装就能试」是这个产品最强的一句话。 */}
            <Link
              href={APP_HOME}
              className="group inline-flex h-12 items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-7 text-sm font-semibold text-[var(--brand-ink)] transition-[transform,box-shadow] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] hover:shadow-[0_16px_40px_-8px_color-mix(in_oklch,var(--brand-solid)_60%,transparent)] active:scale-[0.98] motion-reduce:transition-none"
            >
              在浏览器里直接用
              <ArrowRight className="size-4 transition-transform duration-300 group-hover:translate-x-0.5 motion-reduce:transition-none" />
            </Link>
            {/* 次级按钮在深底上**自带一层底色**，不只靠描边。`border-white/18` 单看只有
                约 1.5:1，够不上 SC 1.4.11 的 3:1；加一层 6% 白底之后，这枚按钮即使描边
                看不清也仍然读得出是一个控件，而不是一段带框的文字。 */}
            <a
              href={links.downloads}
              className="inline-flex h-12 items-center justify-center rounded-xl border border-white/25 bg-white/[0.06] px-7 text-sm font-semibold text-white transition-colors hover:border-white/40 hover:bg-white/[0.12] active:scale-[0.98]"
            >
              下载桌面端
            </a>
          </div>
        </div>

        {/* 截图向右出血：大屏上它被视口切掉一角，读起来像「界面还在继续」，
            而不是一张摆正居中的产品照。玻璃边框是整页唯一一处玻璃——
            DESIGN.md 把玻璃留给结构容器，一张图的外框正好是结构容器。 */}
        <div
          className="anim-rise relative lg:-mr-[16vw] xl:-mr-[12vw]"
          style={{ "--d": "0.2s" } as React.CSSProperties}
        >
          <div className="rounded-[20px] border border-white/12 bg-white/[0.04] p-1.5 shadow-[0_48px_120px_-24px_rgba(0,0,0,0.75)] backdrop-blur-sm">
            <Image
              src={assetPath("shots/desktop-devices.png")}
              alt="SwarmDrop 桌面端的设备页：已配对设备网格，第一张卡显示对端在线、信任级别为协作者、经局域网连接，右侧是附近设备与配对入口。"
              width={1680}
              height={920}
              priority
              className="h-auto w-full rounded-[14px]"
            />
          </div>
        </div>
      </div>
    </section>
  );
}

/* ────────────────────────  PLATFORM STRIP  ──────────────────────── */

const PLATFORMS = ["macOS", "Windows", "Linux", "Android", "浏览器"];

/**
 * hero 与正文之间的一条细带。
 *
 * 它承接的是 hero 里**不允许**放的那类信息（平台清单、版本、协议）——
 * 「hero 只留一个重心，佐证下沉到紧邻的区块」。做成一行细带而不是一排数字方块，
 * 是因为上一版那排 `5 / 0 / 0 / MIT` 恰好是最典型的模板痕迹：
 * 「0 个中央服务器」既不是度量也不是卖点，只是把一句话排成了大字。
 */
function PlatformStrip() {
  return (
    <div className="border-b border-fd-border bg-fd-card/40">
      <div
        className={`${SHELL} flex flex-wrap items-center justify-between gap-x-8 gap-y-3 py-5 text-[13px] text-fd-muted-foreground`}
      >
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
          {PLATFORMS.map((name) => (
            <span key={name} className="font-medium text-fd-foreground/80">
              {name}
            </span>
          ))}
        </div>
        {/* 两项之间**用间距分开，不画分隔符**。此前这里有一枚 `text-fd-border` 的斜杠，
            实测与底色只有 1.21:1——它没被判为无障碍问题（`aria-hidden` 的装饰），
            但也确实看不见，等于花了一个字符什么也没说。 */}
        <div className="flex items-center gap-6">
          <span>开源 · MIT</span>
          <a
            href={links.repo}
            className="inline-flex items-center gap-1.5 transition-colors hover:text-fd-foreground"
          >
            <Github className="size-3.5" />
            源码
          </a>
        </div>
      </div>
    </div>
  );
}

/* ───────────────────────────  ROUTING  ──────────────────────────── */

/**
 * 三条链路是**有序的**：先试局域网，不行打洞，都不行才中继。
 *
 * 这是本页唯一使用序号标记的地方，理由也只有这一条——它真的是一个顺序，序号携带了
 * 「先后」这条信息。给每个区块都编号（01 · 关于 / 02 · 功能）才是模板痕迹。
 */
const ROUTES: Array<{ name: string; how: string; transports: string }> = [
  {
    name: "局域网直连",
    how: "mDNS 在同一网段里发现对方，数据不出这间屋子。浏览器走 WebTransport——它是浏览器上的 QUIC，回环实测是 WebRTC Direct 的 4.5 倍。",
    transports: "TCP · QUIC · WebTransport · WebRTC Direct",
  },
  {
    name: "NAT 打洞",
    how: "两端同时朝对方发包，在中间会合（DCUtR）。跨网络的常态就落在这里。",
    transports: "QUIC · WebRTC",
  },
  {
    name: "中继兜底",
    how: "前两条都不通才走。中继只转发密文，看不到内容，也不留副本。",
    transports: "p2p-circuit",
  },
];

/**
 * 真实地址取自一台正在运行的节点（`get_network_status` 的 `listenAddrs`），
 * **只做截断**。截断处留 `…`：一条把中段悄悄去掉的 multiaddr 仍然像一条完整地址，
 * 会被原样粘进 issue——这条规则写在 DESIGN.md 的 Device Card Contract 里。
 *
 * **这里刻意没有 WebTransport 那条**（形态是
 * `…/udp/4004/quic-v1/webtransport/certhash/<当前>/certhash/<下一张>/p2p/<id>`）：
 * 手头没有一份真实抓下来的 certhash，而这一页的规矩是只截断不编造。它的机制写在下面
 * 那段说明文字里，等抓到真地址再补进这张表。
 */
const REAL_ADDRS: Array<{ label: string; addr: string }> = [
  {
    label: "局域网",
    addr: "/ip4/192.168.50.105/udp/61961/webrtc-direct/certhash/uEiD7XGPe…/p2p/12D3KooW…",
  },
  {
    label: "公网中继",
    addr: "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkaj…/p2p-circuit/p2p/12D3KooW…",
  },
  {
    label: "浏览器入口",
    addr: "/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/uEiBuBPte…/p2p-circuit/p2p/12D3KooW…",
  },
];

function Routing() {
  return (
    <Section>
      <SectionHead
        title="文件走的哪条路，页面上写着"
        lead="多数工具只告诉你「已发送」。SwarmDrop 把链路摊开：哪种传输、对端地址是什么、经不经中继。排查的时候可以整段复制进 issue。"
      />

      {/* **版式刻意不是「左文右盒」的两栏**——那个家族这一页已经给了「agent 也是一台设备」
          区块。这里是「有序清单在上、整幅数据条在下」：链路本来就是有先后的三段，
          而底下那三条 multiaddr 是长字符串，横着铺满比塞进 0.85fr 的窄栏少折好几行。 */}
      <div className="mt-14">
        <ol className="reveal grid gap-x-12 sm:grid-cols-2 lg:grid-cols-3">
          {ROUTES.map((route, i) => (
            <li key={route.name} className="border-t border-fd-border pt-5">
              {/* 序号是这一页唯一一处编号，因为这里真的是顺序：先试局域网，不行打洞，
                  都不行才中继。给每个区块都编号（01 关于 / 02 功能）才是模板痕迹。 */}
              <span className="font-mono text-xs tabular-nums text-[var(--brand)]">
                {String(i + 1).padStart(2, "0")}
              </span>
              <h3 className="mt-3 text-lg font-semibold tracking-tight">{route.name}</h3>
              <p className="mt-2 text-[15px] leading-7 text-fd-muted-foreground">{route.how}</p>
              {/* 不加 `/80`：12px 的次级色本来就贴着 AA，再乘一层透明度实测掉到
                  **3.34:1**。这一行是传输协议名（TCP / QUIC / WebRTC Direct），
                  正是用户会去比对日志的那种字，不该比旁边的说明还淡。 */}
              <p className="mt-3 font-mono text-xs text-fd-muted-foreground">{route.transports}</p>
            </li>
          ))}
        </ol>

        <div className="reveal mt-14 rounded-2xl border border-fd-border bg-fd-card p-5 sm:p-7">
          <p className="text-[13px] font-medium">一台真实节点当前监听的地址</p>
          <p className="mt-1 text-xs text-fd-muted-foreground">
            原样取自运行中的节点，中段已截断。三条分别对应上面三种路径。
          </p>
          {/* 地址是长字符串，`overflow-x-auto` 让它在自己这块里横滚，
              绝不允许把整页顶出横向滚动条。 */}
          <dl className="mt-6 grid gap-5 overflow-x-auto lg:grid-cols-3 lg:gap-8">
            {REAL_ADDRS.map((item) => (
              <div key={item.label} className="min-w-0">
                <dt className="text-xs font-medium text-[var(--brand)]">{item.label}</dt>
                <dd className="mt-1.5">
                  <Mono>{item.addr}</Mono>
                </dd>
              </div>
            ))}
          </dl>
        </div>

        <p className="reveal mt-6 max-w-[72ch] text-[13px] leading-6 text-fd-muted-foreground">
          浏览器有两条入口，各管一段。<Mono>webrtc-direct</Mono>{" "}
          负责发现与打洞——它是唯一能穿透 NAT 的那条，引导节点在 4003 端口上为它开着。
          <Mono>WebTransport</Mono>{" "}
          负责同网快通道，本质是浏览器上的 QUIC；它连的机器没有域名，靠地址里的证书哈希建立信任，
          而那张自签证书的有效期被规范限死在 14 天，所以地址里总是带着两个哈希——当前这张，和下一张。
        </p>
      </div>
    </Section>
  );
}

/* ─────────────────────────  THREE HOSTS  ────────────────────────── */

/**
 * 错位双图。**不做等大三宫格**——那是本页要避开的「相同卡片网格」，
 * 也不是这两张图的真实关系：桌面端是主场，浏览器端是「连这个都能是完整客户端」的证据，
 * 两者权重不等，版面就该不等。
 */
function ThreeHosts() {
  return (
    <Section tone="sunken">
      <SectionHead
        title="三端，同一个内核"
        lead="桌面端与移动端共用同一份 Rust 核心，浏览器端把它编成 wasm——不是「网页版精简功能」，而是完整的节点、协议和配对握手，只少了 mDNS。"
      />

      <div className="mt-14 grid items-end gap-8 lg:grid-cols-12">
        <figure className="reveal lg:col-span-7">
          <div className="overflow-hidden rounded-2xl border border-fd-border shadow-[0_28px_70px_-30px_rgb(15_23_42_/_0.45)]">
            <Image
              src={assetPath("shots/web-devices.png")}
              alt="SwarmDrop 浏览器端的设备页：左侧持久侧边栏，主区是已配对设备与配对面板，设备卡显示在线状态、信任级别与局域网连接延迟。"
              width={2800}
              height={1300}
              className="h-auto w-full"
            />
          </div>
          <figcaption className="mt-4 text-[13px] text-fd-muted-foreground">
            浏览器端 —— 零安装。打开网页就是一个节点，配对、收发、断点续传都在这一页里。
          </figcaption>
        </figure>

        <figure className="reveal lg:col-span-5">
          <div className="overflow-hidden rounded-2xl border border-fd-border shadow-[0_28px_70px_-30px_rgb(15_23_42_/_0.45)]">
            <Image
              src={assetPath("shots/desktop-pairing.png")}
              alt="SwarmDrop 桌面端的配对页：中间是邀请二维码与 24 小时倒计时，左侧列出已发出的邀请及其状态，每条都可撤销。"
              width={1680}
              height={960}
              className="h-auto w-full"
            />
          </div>
          <figcaption className="mt-4 text-[13px] text-fd-muted-foreground">
            桌面端 —— 邀请是一次性的，发出去的每一条都能在这里撤销。
          </figcaption>
        </figure>
      </div>
    </Section>
  );
}

/* ────────────────────────  SECURITY LEDGER  ─────────────────────── */

const SECURITY: Array<{ term: string; detail: ReactNode }> = [
  {
    term: "设备身份",
    detail: (
      <>
        Ed25519 密钥对。移动端交给系统安全存储（iOS Keychain / Android
        EncryptedSharedPreferences）；桌面端存在应用数据目录下仅本人可读的文件里（unix 权限
        0600），形态与无口令的 SSH 私钥一致。
      </>
    ),
  },
  {
    term: "传输加密",
    detail: <>Noise 与 QUIC-TLS，在传输层完成。中继节点全程只经手密文。</>,
  },
  {
    term: "逐块校验",
    detail: (
      <>
        BLAKE3 + bao-tree。每收到一块立刻可验，不必等整个文件收完才发现它被动过；续传也因此不需要「信任对端」。
      </>
    ),
  },
  {
    term: "配对",
    detail: (
      <>
        一次性签名邀请：Ed25519 签名 + 128 位凭证，24 小时有效，用掉即作废。没有可被猜到的短码。
      </>
    ),
  },
  {
    term: "遥测",
    detail: <>没有。不连任何分析后台，也不加载外部字体或脚本。</>,
  },
  {
    term: "源码",
    detail: <>MIT，全部公开。引导节点也可以自建，连发现层都不必依赖别人。</>,
  },
];

/**
 * 定义列表，**没有图标片**。
 *
 * 「圆角方块图标 + 标题 + 灰描述」在上一版首页出现了三个区块共 9 次，是本页要清掉的
 * 主要模板痕迹之一（brand register 明确把「每个标题上方一枚大圆角图标」列为 tell）。
 * 安全条目本来就是「术语 → 解释」，定义列表是它的原生形态，也更密、更好扫。
 */
function SecurityLedger() {
  return (
    <Section>
      <SectionHead
        title="安全模型写在这里，不写在标语里"
        lead="下面每一条都能在源码里对上号。没有「军工级」，也没有「银行级」——那些词不携带任何可核对的信息。"
      />

      <dl className="reveal mt-14 grid gap-x-14 sm:grid-cols-2">
        {SECURITY.map((item) => (
          <div key={item.term} className="border-t border-fd-border py-6">
            <dt className="text-[15px] font-semibold tracking-tight">{item.term}</dt>
            <dd className="mt-2 max-w-[46ch] text-[15px] leading-7 text-fd-muted-foreground">
              {item.detail}
            </dd>
          </div>
        ))}
      </dl>

      {/* 这一块是整页最不像落地页的东西，也是刻意留的。
          安全工具的可信度来自「愿意说自己拆掉了什么、为什么」，而不是来自罗列加密算法。

          强调用**顶边**而不是左侧色条：后者（`border-left` 当装饰色条）是被明令禁止的
          那类 callout 形态。顶边是这一页上「区块用横线分隔」这套语言的延续。 */}
      <div className="reveal mt-6 border-t-2 border-[var(--brand)] bg-fd-card/50 p-6 sm:p-8">
        <h3 className="text-lg font-semibold tracking-tight">我们删掉过一层加密</h3>
        <div className="mt-3 max-w-[62ch] text-[15px] leading-7 text-fd-muted-foreground">
          <p>
            早期版本在传输层之上又套了一层 XChaCha20-Poly1305，后来整块删了。它是自引用的冗余——
            密钥经同一条 Noise 信道分发，能读密文的攻击者必然也能读密钥；更要命的是它和逐块校验
            不能共存，加密之后校验和会变成密文的哈希，「根哈希 == 明文 BLAKE3」这条不变量当场就塌。
            宁可少一层，也不要一层看起来很安全的装饰。
          </p>
        </div>
        <Link
          href="/docs"
          className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-[var(--brand)] transition-opacity hover:opacity-80"
        >
          完整推导在文档里
          <ArrowRight className="size-3.5" />
        </Link>
      </div>
    </Section>
  );
}

/* ────────────────────────────  AGENTS  ──────────────────────────── */

/** 20 个工具里挑 6 个有代表性的。列全 20 条是数据倾倒，不是信息。 */
const MCP_TOOLS = [
  "send_files",
  "list_paired_devices",
  "search_inbox",
  "get_transfer_status",
  "accept_transfer",
  "export_inbox_item",
];

const MCP_CONFIG = `{
  "mcpServers": {
    "swarmdrop": {
      "url": "http://127.0.0.1:19527"
    }
  }
}`;

function Agents() {
  return (
    <Section tone="sunken">
      <div className="grid gap-x-14 gap-y-12 lg:grid-cols-[minmax(0,1fr)_minmax(0,0.9fr)] lg:items-center">
        <div>
          <SectionHead
            title="agent 也是一台设备"
            lead="桌面端内置一个本地 MCP server，20 个工具。agent 用的是和你完全相同的那条通道——发文件、查收件箱、看传输状态。"
          />

          <ul className="reveal mt-8 flex flex-wrap gap-2">
            {MCP_TOOLS.map((tool) => (
              <li
                key={tool}
                className="rounded-full border border-fd-border bg-fd-card px-3 py-1 font-mono text-xs text-fd-muted-foreground"
              >
                {tool}
              </li>
            ))}
            <li className="px-1 py-1 text-xs text-fd-muted-foreground">共 20 个</li>
          </ul>

          {/* 这条是实测出来的，写在这里因为它正好回答了「让 AI 碰我的文件安全吗」：
              权限是按设备给的，而且默认不给。 */}
          <p className="reveal mt-6 max-w-[52ch] text-[15px] leading-7 text-fd-muted-foreground">
            每台已配对设备单独控制允不允许 MCP 向它发送，
            <span className="font-medium text-fd-foreground">默认关闭</span>
            。没开的时候 agent 收到的是一句明确的拒绝，而不是一次静默的发送。
          </p>
        </div>

        <div className="reveal">
          <div className="overflow-hidden rounded-2xl border border-fd-border bg-fd-card">
            <div className="border-b border-fd-border px-5 py-3 font-mono text-xs text-fd-muted-foreground">
              claude_desktop_config.json
            </div>
            <pre className="overflow-x-auto px-5 py-5">
              <code className="font-mono text-[13px] leading-6">{MCP_CONFIG}</code>
            </pre>
          </div>
          <p className="mt-4 text-xs leading-6 text-fd-muted-foreground">
            端口在设置页里可改；服务只监听 <Mono>127.0.0.1</Mono>，不对外暴露。
          </p>
        </div>
      </div>
    </Section>
  );
}

/* ───────────────────────────  DOWNLOADS  ────────────────────────── */

/**
 * 桌面与移动**合并成一个区块**。
 *
 * 上一版是两个各自带标题的灰盒子上下堆着，各说一遍「stable 最新版本」，
 * 占掉页面中段一大半。两张卡现在并排，标题只说一次。
 */
function Downloads({
  catalog,
  mobileCatalog,
}: {
  catalog: DownloadCatalog | null;
  mobileCatalog: DownloadCatalog | null;
}) {
  const allowClientRefresh = swarmhiveConfig.baseUrl.startsWith("https://");

  return (
    <Section id="download">
      <SectionHead
        title="装到你的设备上"
        lead="安装包由我们自己的开源发布服务分发，装好之后应用会自己检查更新。"
      />

      <div className="reveal mt-12 grid gap-6 lg:grid-cols-[minmax(0,1.55fr)_minmax(0,1fr)] lg:items-start">
        <DownloadPanel
          baseUrl={swarmhiveConfig.baseUrl}
          appSlug={swarmhiveConfig.appSlug}
          channel={swarmhiveConfig.channel}
          fallbackUrl={links.releases}
          initialCatalog={catalog}
          allowClientRefresh={allowClientRefresh}
        />
        <MobileDownloadCard
          baseUrl={swarmhiveConfig.baseUrl}
          appSlug={swarmhiveConfig.appSlugMobile}
          channel={swarmhiveConfig.channel}
          fallbackUrl={links.mobile}
          initialCatalog={mobileCatalog}
          allowClientRefresh={allowClientRefresh}
        />
      </div>
    </Section>
  );
}

/* ───────────────────────────  CLOSING  ──────────────────────────── */

/**
 * 收尾用**品牌色整幅铺满**，不用「深色圆角大盒子 + 中心白色径向光」——后者是
 * 落地页收尾区最标准的那个模板（上一版正是它）。整幅铺满也让页面在结束处
 * 有一次明确的换气，而不是又一张卡片。
 */
function Closing() {
  return (
    <Section tone="brand">
      <div className="flex flex-col items-start gap-8 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <h2 className="max-w-[18ch] text-balance text-[clamp(1.9rem,3.8vw,3rem)] font-bold leading-[1.1] tracking-[-0.03em]">
            配一次对，之后它就一直在那儿。
          </h2>
          {/* 「接收是后台默认行为」是 PRODUCT.md 的第一条设计原则，整页只在这里说一次——
              它是收尾处最该留下的那句：用户真正要的不是「一个传文件的 App」，
              是「不用再想传文件这件事」。 */}
          {/* **这里不能用 `opacity-80`**：品牌青绿底 + 深墨字满强度是 4.98:1，压到 80%
              就掉到 **4.08**，低于 AA——而在实心品牌色上，任何「让它淡一点」的写法都会
              直接吃掉本就不宽裕的那点余量（DESIGN.md：deep ink on teal ≈ 5.0，就是全部）。
              层次靠字号给：48px 粗体标题 vs 15px 正文，已经足够。 */}
          <p className="mt-4 max-w-[46ch] text-[15px] leading-7">
            没有「发送模式」和「接收模式」——配过对的设备发来东西，后台就收下了。
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <Link
            href={APP_HOME}
            className="group inline-flex h-12 items-center justify-center gap-2 rounded-xl bg-[var(--brand-ink)] px-7 text-sm font-semibold text-white transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] active:scale-[0.98] motion-reduce:transition-none"
          >
            在浏览器里直接用
            <ArrowRight className="size-4 transition-transform duration-300 group-hover:translate-x-0.5 motion-reduce:transition-none" />
          </Link>
          <a
            href={links.repo}
            className="inline-flex h-12 items-center justify-center gap-2 rounded-xl border border-[var(--brand-ink)]/25 px-7 text-sm font-semibold transition-colors hover:bg-[var(--brand-ink)]/10 active:scale-[0.98]"
          >
            <Github className="size-4" />
            查看源码
          </a>
        </div>
      </div>
    </Section>
  );
}

/* ────────────────────────────  FOOTER  ──────────────────────────── */

/**
 * 页脚上方多一条状态带。
 *
 * **它不该出现在 hero 里**：hero 只留一个重心，而「还在打磨」这种限定语放在首屏
 * 会先给产品打个折。但它也不能不说——首页最重要的属性是说的话是真的，而这个项目
 * 确实处在「功能齐了、细节在磨」的阶段。放在页脚上方：想下载的人早就点走了，
 * 读到这里的人正是会去开 issue 的那批。
 */
function StatusNote() {
  return (
    <div className="border-t border-fd-border bg-fd-card/40">
      <div className={`${SHELL} py-7`}>
        <p className="max-w-[74ch] text-[13px] leading-6 text-fd-muted-foreground">
          <span className="font-medium text-fd-foreground">项目现状：</span>
          核心功能已经完成并稳定跑了一段时间——跨网络传输、配对、断点续传、三端客户端、MCP。
          当前的工作是收尾打磨：三端交互对齐、边界状态的文案、真机链路的收敛（尤其是跨网络那条）。
          用起来有别扭或看不懂的地方，
          <a
            href={`${links.repo}/issues`}
            className="text-[var(--brand)] transition-opacity hover:opacity-80"
          >
            开一个 issue
          </a>
          是这个阶段最有价值的反馈。
        </p>
      </div>
    </div>
  );
}

function Footer() {
  return (
    <footer className="border-t border-fd-border">
      <div
        className={`${SHELL} flex flex-col items-center justify-between gap-4 py-8 text-sm text-fd-muted-foreground sm:flex-row`}
      >
        <span>© {appName} Contributors · MIT License</span>
        <div className="flex items-center gap-6">
          <Link href="/docs" className="transition-colors hover:text-fd-foreground">
            文档
          </Link>
          <a href={links.downloads} className="transition-colors hover:text-fd-foreground">
            下载
          </a>
          <a
            href={links.repo}
            className="inline-flex items-center gap-1.5 transition-colors hover:text-fd-foreground"
          >
            <Github className="size-4" />
            GitHub
          </a>
        </div>
      </div>
    </footer>
  );
}
