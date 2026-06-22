import {
  ArrowRight,
  Fingerprint,
  Github,
  KeyRound,
  Languages,
  Lock,
  MonitorSmartphone,
  Network,
  Radio,
  Route,
  ServerOff,
  ShieldCheck,
  Webhook,
} from "lucide-react";
import Link from "next/link";
import type { ReactNode } from "react";
import { SwarmVisual } from "@/components/swarm-visual";
import { appName, links } from "@/lib/shared";

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col overflow-hidden">
      <Hero />
      <Stats />
      <Features />
      <HowItWorks />
      <Security />
      <FinalCta />
      <Footer />
    </main>
  );
}

/* ─────────────────────────────  HERO  ───────────────────────────── */

function Hero() {
  return (
    <section className="relative border-b border-fd-border">
      {/* 品牌径向辉光（品牌蓝，非 AI 紫），双模自适应 */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 opacity-70"
        style={{
          background:
            "radial-gradient(60% 55% at 78% 30%, color-mix(in oklch, var(--brand) 22%, transparent), transparent 70%)",
        }}
      />
      <div className="mx-auto grid w-full max-w-6xl items-center gap-10 px-6 py-16 lg:grid-cols-[1.05fr_0.95fr] lg:py-24">
        {/* 左：文案 */}
        <div className="flex flex-col items-start">
          <span
            className="anim-rise mb-6 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card/70 px-3 py-1 text-xs font-medium text-fd-muted-foreground backdrop-blur"
            style={{ "--d": "0s" } as React.CSSProperties}
          >
            <ShieldCheck className="size-3.5 text-[var(--brand)]" strokeWidth={2.25} />
            去中心化 · 端到端加密
          </span>
          <h1
            className="anim-rise max-w-xl text-balance text-4xl font-bold leading-[1.08] tracking-tight sm:text-5xl lg:text-6xl"
            style={{ "--d": "0.06s" } as React.CSSProperties}
          >
            把文件丢到任何地方。
            <br />
            <span className="text-[var(--brand)]">不上云，不限速。</span>
          </h1>
          <p
            className="anim-rise mt-6 max-w-md text-pretty text-lg text-fd-muted-foreground"
            style={{ "--d": "0.12s" } as React.CSSProperties}
          >
            {appName} 让设备点对点直连，文件只有收发双方能解密。无账号，无服务器，跨任意网络。
          </p>
          <div
            className="anim-rise mt-9 flex flex-wrap items-center gap-3"
            style={{ "--d": "0.18s" } as React.CSSProperties}
          >
            <a
              href={links.releases}
              className="group inline-flex h-12 items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-7 text-sm font-semibold text-white shadow-sm transition-all hover:opacity-90 hover:shadow-md active:scale-[0.98]"
            >
              下载 {appName}
              <ArrowRight className="size-4 transition-transform group-hover:translate-x-0.5" />
            </a>
            <Link
              href="/docs"
              className="inline-flex h-12 items-center justify-center rounded-xl border border-fd-border bg-fd-card px-7 text-sm font-semibold transition-colors hover:bg-fd-accent hover:text-fd-accent-foreground active:scale-[0.98]"
            >
              阅读文档
            </Link>
          </div>
        </div>

        {/* 右：蜂群可视化 */}
        <div
          className="anim-rise relative mx-auto aspect-square w-full max-w-md"
          style={{ "--d": "0.22s" } as React.CSSProperties}
        >
          <SwarmVisual />
        </div>
      </div>
    </section>
  );
}

/* ─────────────────────────────  STATS  ──────────────────────────── */

const STATS: Array<{ value: string; label: string }> = [
  { value: "5", label: "平台支持" },
  { value: "0", label: "中央服务器" },
  { value: "0", label: "账号注册" },
  { value: "MIT", label: "开源协议" },
];

function Stats() {
  return (
    <section className="border-b border-fd-border bg-fd-card/30">
      <div className="mx-auto grid w-full max-w-6xl grid-cols-2 divide-fd-border px-6 py-10 sm:grid-cols-4 sm:divide-x">
        {STATS.map((s) => (
          <div key={s.label} className="reveal flex flex-col items-center gap-1 px-2 py-3 text-center">
            <span className="text-3xl font-bold tracking-tight text-[var(--brand)]">{s.value}</span>
            <span className="text-sm text-fd-muted-foreground">{s.label}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

/* ───────────────────────────  FEATURES  ─────────────────────────── */

const ROUTES: Array<{ icon: ReactNode; name: string; latency: string }> = [
  { icon: <Radio className="size-4" />, name: "局域网直连", latency: "~2 ms" },
  { icon: <Route className="size-4" />, name: "NAT 打洞 (DCUtR)", latency: "10-100 ms" },
  { icon: <Network className="size-4" />, name: "中继转发兜底", latency: "100-500 ms" },
];

function Features() {
  return (
    <section className="mx-auto w-full max-w-6xl px-6 py-20">
      <div className="reveal mb-12 max-w-2xl">
        <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">
          一套连接，自己找到最快的路
        </h2>
        <p className="mt-3 text-fd-muted-foreground">
          从同一间办公室到地球另一端，{appName} 在局域网、NAT 打洞与中继之间自动选优，全程加密。
        </p>
      </div>

      <div className="grid gap-4 lg:grid-cols-6">
        {/* 端到端加密（tinted） */}
        <Cell className="lg:col-span-2 bg-[var(--brand)]/[0.06]">
          <CellIcon>
            <Lock className="size-5" />
          </CellIcon>
          <CellTitle>端到端加密</CellTitle>
          <CellDesc>
            每次传输独立生成 256-bit 密钥，XChaCha20-Poly1305 加密。引导节点、中继都看不到明文。
          </CellDesc>
        </Cell>

        {/* 跨网络自动选路（brand gradient，含路由清单） */}
        <Cell className="relative overflow-hidden lg:col-span-4">
          <div
            aria-hidden
            className="pointer-events-none absolute -right-16 -top-16 size-56 rounded-full opacity-60"
            style={{
              background:
                "radial-gradient(circle, color-mix(in oklch, var(--sky) 30%, transparent), transparent 70%)",
            }}
          />
          <CellIcon>
            <Route className="size-5" />
          </CellIcon>
          <CellTitle>跨网络自动选路</CellTitle>
          <CellDesc>mDNS 发现局域网设备，分享码经 DHT 定位对端，DCUtR 打洞，失败再走中继。</CellDesc>
          <ul className="mt-5 grid gap-2 sm:grid-cols-3">
            {ROUTES.map((r) => (
              <li
                key={r.name}
                className="flex flex-col gap-1 rounded-lg border border-fd-border bg-fd-card/70 p-3"
              >
                <span className="flex items-center gap-2 text-sm font-medium text-[var(--brand)]">
                  {r.icon}
                  {r.name}
                </span>
                <span className="font-mono text-xs text-fd-muted-foreground">{r.latency}</span>
              </li>
            ))}
          </ul>
        </Cell>

        {/* 零配置 */}
        <Cell className="lg:col-span-2">
          <CellIcon>
            <ServerOff className="size-5" />
          </CellIcon>
          <CellTitle>零配置无服务器</CellTitle>
          <CellDesc>不要账号，不要中央服务器。装好即用，数据只在你的设备之间流动。</CellDesc>
        </Cell>

        {/* 全平台 */}
        <Cell className="lg:col-span-2 bg-[var(--brand)]/[0.06]">
          <CellIcon>
            <MonitorSmartphone className="size-5" />
          </CellIcon>
          <CellTitle>全平台覆盖</CellTitle>
          <CellDesc>Windows · macOS · Linux 桌面端，Android 移动端共用同一套 Rust 核心。</CellDesc>
        </Cell>

        {/* 自托管 + 开源 */}
        <Cell className="lg:col-span-2">
          <CellIcon>
            <Github className="size-5" />
          </CellIcon>
          <CellTitle>开源可自托管</CellTitle>
          <CellDesc>MIT 协议，代码全公开。可自建引导节点，连发现层都不必依赖别人。</CellDesc>
        </Cell>
      </div>
    </section>
  );
}

function Cell({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`reveal flex flex-col rounded-2xl border border-fd-border bg-fd-card p-6 transition-colors hover:border-[var(--brand)]/40 ${className}`}
    >
      {children}
    </div>
  );
}
function CellIcon({ children }: { children: ReactNode }) {
  return (
    <div className="mb-4 inline-flex size-10 items-center justify-center rounded-xl bg-[var(--brand)]/10 text-[var(--brand)]">
      {children}
    </div>
  );
}
function CellTitle({ children }: { children: ReactNode }) {
  return <h3 className="mb-1.5 text-lg font-semibold">{children}</h3>;
}
function CellDesc({ children }: { children: ReactNode }) {
  return <p className="text-sm leading-relaxed text-fd-muted-foreground">{children}</p>;
}

/* ─────────────────────────  HOW IT WORKS  ───────────────────────── */

const STEPS: Array<{ title: string; desc: string }> = [
  { title: "启动节点", desc: "设一个安全密码，开启本地 P2P 节点。私钥落 Stronghold 加密保险库。" },
  { title: "配对设备", desc: "跨网络输入 6 位配对码，同一 Wi-Fi 自动发现。一次配对，长期信任。" },
  { title: "拖拽发送", desc: "选中对方设备，把文件拖进窗口。加密直传，实时进度，完成即通知。" },
];

function HowItWorks() {
  return (
    <section className="border-y border-fd-border bg-fd-card/30">
      <div className="mx-auto w-full max-w-6xl px-6 py-20">
        <h2 className="reveal mb-12 text-3xl font-bold tracking-tight sm:text-4xl">三步开始传输</h2>
        <ol className="grid gap-8 md:grid-cols-3">
          {STEPS.map((s, i) => (
            <li key={s.title} className="reveal relative flex flex-col">
              <div className="mb-4 flex items-center gap-3">
                <span className="inline-flex size-9 items-center justify-center rounded-full bg-[var(--brand-solid)] font-mono text-sm font-bold text-white">
                  {i + 1}
                </span>
                {i < STEPS.length - 1 && (
                  <span className="hidden h-px flex-1 bg-gradient-to-r from-[var(--brand)]/50 to-transparent md:block" />
                )}
              </div>
              <h3 className="mb-2 text-xl font-semibold">{s.title}</h3>
              <p className="text-sm leading-relaxed text-fd-muted-foreground">{s.desc}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

/* ───────────────────────────  SECURITY  ─────────────────────────── */

const SECURITY: Array<{ icon: ReactNode; title: string; desc: string }> = [
  {
    icon: <KeyRound className="size-5" />,
    title: "设备身份 Ed25519",
    desc: "每台设备一对密钥，私钥永不离开 Stronghold 加密保险库。",
  },
  {
    icon: <ShieldCheck className="size-5" />,
    title: "一次一密",
    desc: "每次传输独立生成 256-bit 对称密钥，XChaCha20-Poly1305。",
  },
  {
    icon: <Fingerprint className="size-5" />,
    title: "生物识别解锁",
    desc: "TouchID / FaceID / Windows Hello，本地校验，密钥不出设备。",
  },
  {
    icon: <Webhook className="size-5" />,
    title: "零遥测",
    desc: "不收集任何用户数据，不连分析后台。连接全在你掌控之中。",
  },
];

function Security() {
  return (
    <section className="mx-auto grid w-full max-w-6xl gap-12 px-6 py-20 lg:grid-cols-2">
      <div className="reveal">
        <span className="mb-4 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs font-medium text-fd-muted-foreground">
          <Lock className="size-3.5 text-[var(--brand)]" strokeWidth={2.25} />
          安全模型
        </span>
        <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">
          中继也帮不上忙，因为它看不到内容
        </h2>
        <p className="mt-4 max-w-md text-fd-muted-foreground">
          所有加密在收发两端完成。哪怕流量经过中继节点，密文之外什么都拿不到。
        </p>
        <div className="mt-6 flex items-center gap-2 text-sm text-fd-muted-foreground">
          <Languages className="size-4 text-[var(--brand)]" />
          已支持简体中文 · 繁体中文 · English
        </div>
      </div>

      <ul className="grid gap-px overflow-hidden rounded-2xl border border-fd-border bg-fd-border sm:grid-cols-2">
        {SECURITY.map((s) => (
          <li key={s.title} className="reveal flex flex-col bg-fd-card p-6">
            <div className="mb-3 inline-flex size-10 items-center justify-center rounded-xl bg-[var(--brand)]/10 text-[var(--brand)]">
              {s.icon}
            </div>
            <h3 className="mb-1.5 font-semibold">{s.title}</h3>
            <p className="text-sm leading-relaxed text-fd-muted-foreground">{s.desc}</p>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ──────────────────────────  FINAL CTA  ─────────────────────────── */

function FinalCta() {
  return (
    <section className="mx-auto w-full max-w-6xl px-6 pb-20">
      <div className="relative overflow-hidden rounded-3xl border border-fd-border bg-[var(--brand-solid)] px-8 py-16 text-center">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 opacity-40"
          style={{
            background:
              "radial-gradient(50% 80% at 50% 0%, color-mix(in oklch, white 25%, transparent), transparent 70%)",
          }}
        />
        <h2 className="relative mx-auto max-w-2xl text-balance text-3xl font-bold tracking-tight text-white sm:text-4xl">
          准备好把文件丢出去了吗
        </h2>
        <p className="relative mx-auto mt-4 max-w-md text-balance text-white/80">
          免费开源，所有平台都能装。两分钟跑起你的第一台节点。
        </p>
        <div className="relative mt-9 flex flex-wrap items-center justify-center gap-3">
          <a
            href={links.releases}
            className="inline-flex h-12 items-center justify-center gap-2 rounded-xl bg-white px-7 text-sm font-semibold text-[var(--brand-solid)] shadow-sm transition-transform hover:shadow-md active:scale-[0.98]"
          >
            下载 {appName}
            <ArrowRight className="size-4" />
          </a>
          <a
            href={links.repo}
            className="inline-flex h-12 items-center justify-center gap-2 rounded-xl border border-white/30 px-7 text-sm font-semibold text-white transition-colors hover:bg-white/10 active:scale-[0.98]"
          >
            <Github className="size-4" />
            查看源码
          </a>
        </div>
      </div>
    </section>
  );
}

/* ────────────────────────────  FOOTER  ──────────────────────────── */

function Footer() {
  return (
    <footer className="border-t border-fd-border">
      <div className="mx-auto flex w-full max-w-6xl flex-col items-center justify-between gap-4 px-6 py-8 text-sm text-fd-muted-foreground sm:flex-row">
        <span>
          © {appName} Contributors · MIT License
        </span>
        <div className="flex items-center gap-6">
          <Link href="/docs" className="transition-colors hover:text-fd-foreground">
            文档
          </Link>
          <a href={links.releases} className="transition-colors hover:text-fd-foreground">
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
