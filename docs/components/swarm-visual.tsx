import type { CSSProperties } from "react";

// 首页 hero 的 P2P 蜂群可视化：节点（六边形）+ 连线 + 沿路径飞行的「纸飞机」数据包，
// 直接表达产品核心：设备点对点直连、文件加密后在节点间「投递」。纯 SVG + CSS 动效，
// 无 JS、可静态导出；prefers-reduced-motion 下数据包静止、连线不流动，仍是一张清晰的网络图。

type Node = { cx: number; cy: number; r: number; label?: string; primary?: boolean };

const NODES: Node[] = [
  { cx: 230, cy: 195, r: 34, primary: true }, // 你的设备
  { cx: 78, cy: 96, r: 24 },
  { cx: 372, cy: 86, r: 22 },
  { cx: 66, cy: 300, r: 22 },
  { cx: 392, cy: 296, r: 26 },
  { cx: 236, cy: 40, r: 18 },
];

// 中心到各对端的连线
const LINKS: Array<[number, number]> = [
  [0, 1],
  [0, 2],
  [0, 3],
  [0, 4],
  [0, 5],
  [1, 3],
  [2, 4],
];

// 数据包飞行路径（贝塞尔曲线，offset-path 用）
const PACKETS: Array<{ path: string; d: string }> = [
  { path: "M78 96 Q170 120 230 195", d: "0s" },
  { path: "M392 296 Q300 250 230 195", d: "1.2s" },
  { path: "M230 195 Q320 130 372 86", d: "2.1s" },
];

function hexPoints(cx: number, cy: number, r: number): string {
  const pts: string[] = [];
  for (let i = 0; i < 6; i++) {
    const a = (Math.PI / 180) * (60 * i - 90);
    pts.push(`${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`);
  }
  return pts.join(" ");
}

// 纸飞机（取自品牌图标），随路径切线旋转
function Plane({ path, delay }: { path: string; delay: string }) {
  const style: CSSProperties = {
    offsetPath: `path("${path}")`,
    offsetRotate: "auto",
    // @ts-expect-error CSS 自定义属性
    "--d": delay,
  };
  return (
    <g className="packet" style={style}>
      <path d="M14 0 L-9 8 L-1 0 L-9 -8 Z" fill="var(--sky)" />
      <path d="M14 0 L-1 0 L-9 8 Z" fill="var(--brand-strong)" />
    </g>
  );
}

export function SwarmVisual() {
  return (
    <svg
      viewBox="0 0 460 360"
      className="h-full w-full"
      role="img"
      aria-label="SwarmDrop 设备组成的去中心化网络，文件在节点间点对点加密投递"
    >
      <defs>
        <radialGradient id="core" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--brand-strong)" stopOpacity="0.28" />
          <stop offset="100%" stopColor="var(--brand)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* 中心光晕 */}
      <circle cx="230" cy="195" r="150" fill="url(#core)" />

      {/* 连线 */}
      {LINKS.map(([a, b]) => {
        const n1 = NODES[a];
        const n2 = NODES[b];
        return (
          <line
            key={`${a}-${b}`}
            x1={n1.cx}
            y1={n1.cy}
            x2={n2.cx}
            y2={n2.cy}
            stroke="var(--brand)"
            strokeOpacity={0.32}
            strokeWidth={1.4}
            className={a === 0 ? "link-flow" : undefined}
          />
        );
      })}

      {/* 节点 */}
      {NODES.map((n, i) => (
        <g key={i} className={n.primary ? undefined : "node-pulse"} style={{ "--d": `${i * 0.4}s` } as CSSProperties}>
          <polygon
            points={hexPoints(n.cx, n.cy, n.r)}
            fill={n.primary ? "var(--brand)" : "var(--brand-soft)"}
            fillOpacity={n.primary ? 0.16 : 0.5}
            stroke={n.primary ? "var(--brand-strong)" : "var(--brand)"}
            strokeWidth={n.primary ? 2.2 : 1.6}
            strokeLinejoin="round"
          />
          {n.primary && (
            <polygon
              points={hexPoints(n.cx, n.cy, n.r - 11)}
              fill="none"
              stroke="var(--brand-strong)"
              strokeOpacity={0.6}
              strokeWidth={1.4}
              strokeLinejoin="round"
            />
          )}
        </g>
      ))}

      {/* 飞行的数据包 */}
      {PACKETS.map((p, i) => (
        <Plane key={i} path={p.path} delay={p.d} />
      ))}
    </svg>
  );
}
