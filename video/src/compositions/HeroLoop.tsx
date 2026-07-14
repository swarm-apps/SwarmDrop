import { AbsoluteFill, Easing, interpolate, useCurrentFrame } from "remotion";

type Point = { x: number; y: number; r: number };

const nodes: Point[] = [
  { x: 960, y: 540, r: 80 },
  { x: 410, y: 320, r: 43 },
  { x: 570, y: 785, r: 48 },
  { x: 1370, y: 265, r: 40 },
  { x: 1510, y: 730, r: 54 },
  { x: 945, y: 170, r: 34 },
];

const clamp = (frame: number, from: number, to: number) =>
  interpolate(frame, [from, to], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });

const fadeOut = (frame: number, from: number, to: number) => 1 - clamp(frame, from, to);

function Background({ opacity = 1 }: { opacity?: number }) {
  const frame = useCurrentFrame();

  return (
    <>
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity,
          background: "radial-gradient(circle at 50% 42%, #183f80 0%, #091c3e 42%, #040b19 100%)",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: -230,
          top: -280,
          width: 920,
          height: 920,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(58, 151, 255, .27), transparent 68%)",
          opacity: opacity * 0.9,
          translate: `${interpolate(frame, [0, 600], [0, 120])}px ${interpolate(frame, [0, 600], [0, 54])}px`,
        }}
      />
      <div
        style={{
          position: "absolute",
          right: -270,
          bottom: -330,
          width: 950,
          height: 950,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(33, 213, 178, .19), transparent 68%)",
          opacity,
          translate: `${interpolate(frame, [0, 600], [0, -85])}px ${interpolate(frame, [0, 600], [0, -38])}px`,
        }}
      />
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity: opacity * 0.14,
          backgroundImage:
            "linear-gradient(rgba(156, 211, 255, .4) 1px, transparent 1px), linear-gradient(90deg, rgba(156, 211, 255, .4) 1px, transparent 1px)",
          backgroundSize: "76px 76px",
          maskImage: "radial-gradient(ellipse at center, black, transparent 76%)",
        }}
      />
    </>
  );
}

function NetworkScene() {
  const frame = useCurrentFrame();
  const opacity = clamp(frame, 0, 20) * fadeOut(frame, 92, 120);
  const coreScale = interpolate(frame, [0, 38], [0.72, 1], {
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });

  return (
    <AbsoluteFill style={{ opacity }}>
      <Background opacity={opacity} />
      <div
        style={{
          position: "absolute",
          left: 420,
          top: 88,
          color: "#b7dcff",
          fontSize: 27,
          fontWeight: 750,
          letterSpacing: 8,
          opacity: clamp(frame, 8, 30),
        }}
      >
        SWARMDROP
      </div>
      <svg width="1920" height="1080" style={{ position: "absolute", inset: 0 }}>
        <defs>
          <radialGradient id="network-core">
            <stop offset="0" stopColor="#4cb2ff" stopOpacity=".3" />
            <stop offset="1" stopColor="#4cb2ff" stopOpacity="0" />
          </radialGradient>
        </defs>
        <circle cx="960" cy="540" r="285" fill="url(#network-core)" opacity={opacity} />
        {nodes.slice(1).map((node, index) => (
          <line
            key={`${node.x}-${node.y}`}
            x1="960"
            y1="540"
            x2={node.x}
            y2={node.y}
            stroke="#77c4ff"
            strokeDasharray="9 18"
            strokeOpacity={opacity * (0.28 + clamp(frame, 12 + index * 7, 40 + index * 7) * 0.32)}
            strokeWidth="3"
          />
        ))}
        {nodes.slice(1).map((node, index) => {
          const travel = ((frame - 28 - index * 13) % 88) / 88;
          const x = 960 + (node.x - 960) * Math.max(0, travel);
          const y = 540 + (node.y - 540) * Math.max(0, travel);
          return <circle key={`packet-${node.x}`} cx={x} cy={y} r="7" fill="#d9f3ff" opacity={opacity * clamp(frame, 24 + index * 6, 45 + index * 6)} />;
        })}
      </svg>
      {nodes.map((node, index) => {
        const enter = clamp(frame, index * 7, index * 7 + 25);
        const isCore = index === 0;
        return (
          <div
            key={`${node.x}-${node.y}`}
            style={{
              position: "absolute",
              left: node.x,
              top: node.y,
              display: "grid",
              width: node.r * 2,
              height: node.r * 2,
              placeItems: "center",
              border: `${isCore ? 2.5 : 1.5}px solid ${isCore ? "#9edbff" : "rgba(135, 205, 255, .75)"}`,
              borderRadius: 30,
              background: isCore ? "linear-gradient(145deg, rgba(44, 130, 220, .92), rgba(11, 45, 102, .92))" : "rgba(41, 112, 192, .35)",
              boxShadow: isCore ? "0 0 70px rgba(78, 170, 255, .55)" : "0 0 32px rgba(91, 182, 255, .24)",
              color: "#e9f6ff",
              fontSize: isCore ? 40 : 23,
              fontWeight: 800,
              opacity: opacity * enter,
              translate: "-50% -50%",
              scale: enter * (isCore ? coreScale : 1),
              rotate: `${isCore ? 0 : 8}deg`,
            }}
          >
            {isCore ? "S" : "◇"}
          </div>
        );
      })}
      <div
        style={{
          position: "absolute",
          left: "50%",
          bottom: 120,
          color: "#f4f9ff",
          fontSize: 66,
          fontWeight: 800,
          letterSpacing: -2,
          opacity: clamp(frame, 34, 60),
          translate: "-50% 0",
        }}
      >
        设备，自由相连
      </div>
    </AbsoluteFill>
  );
}

function DeviceRow({ name, icon }: { name: string; icon: string }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 13,
        padding: "14px 16px",
        border: "1px solid rgba(151, 205, 255, .13)",
        borderRadius: 14,
        background: "rgba(9, 31, 68, .68)",
      }}
    >
      <span style={{ display: "grid", width: 29, height: 29, placeItems: "center", borderRadius: 9, background: "rgba(100, 180, 255, .16)", color: "#a8dbff", fontSize: 16 }}>{icon}</span>
      <span style={{ color: "#e8f3ff", fontSize: 18, fontWeight: 650 }}>{name}</span>
      <span style={{ marginLeft: "auto", color: "#7de2a9", fontSize: 15 }}>● 在线</span>
    </div>
  );
}

function ProductScene() {
  const frame = useCurrentFrame();
  const local = frame - 105;
  const opacity = clamp(local, 0, 24) * fadeOut(local, 205, 230);

  return (
    <AbsoluteFill style={{ opacity }}>
      <Background opacity={opacity} />
      <div
        style={{
          position: "absolute",
          top: 82,
          left: "50%",
          color: "#f4f9ff",
          fontSize: 62,
          fontWeight: 800,
          letterSpacing: -2,
          opacity: clamp(local, 8, 30),
          translate: "-50% 0",
        }}
      >
        你的设备，彼此可见
      </div>
      <div
        style={{
          position: "absolute",
          left: 230,
          top: 255,
          width: 710,
          padding: 19,
          border: "1px solid rgba(168, 216, 255, .28)",
          borderRadius: 30,
          background: "linear-gradient(145deg, rgba(22, 68, 133, .94), rgba(5, 23, 57, .94))",
          boxShadow: "-28px 42px 90px rgba(0, 0, 0, .35), inset 0 1px rgba(255, 255, 255, .12)",
          opacity: clamp(local, 12, 45),
          translate: `${interpolate(local, [0, 45], [-110, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px ${interpolate(local, [0, 45], [40, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px`,
          rotate: "-4deg",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 13, padding: "5px 6px 19px" }}>
          <div style={{ display: "grid", width: 43, height: 43, placeItems: "center", borderRadius: 13, background: "linear-gradient(145deg, #77c8ff, #176ad5)", color: "#fff", fontSize: 20, fontWeight: 800 }}>S</div>
          <div>
            <div style={{ color: "#f7fbff", fontSize: 23, fontWeight: 750 }}>SwarmDrop</div>
            <div style={{ marginTop: 4, color: "#83b5df", fontSize: 14 }}>已配对设备</div>
          </div>
        </div>
        <div style={{ display: "grid", gap: 10 }}>
          <DeviceRow name="iPhone" icon="▯" />
          <DeviceRow name="Pixel 7" icon="▯" />
          <DeviceRow name="Windows 工作站" icon="▣" />
        </div>
      </div>
      <div
        style={{
          position: "absolute",
          left: 1210,
          top: 190,
          width: 330,
          height: 680,
          padding: 13,
          borderRadius: 51,
          background: "linear-gradient(145deg, #dae9ff, #647da6)",
          boxShadow: "28px 42px 90px rgba(0, 0, 0, .42), inset 1px 1px rgba(255, 255, 255, .85)",
          opacity: clamp(local, 28, 65),
          translate: `${interpolate(local, [0, 65], [120, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px ${interpolate(local, [0, 65], [-55, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px`,
          rotate: "7deg",
        }}
      >
        <div style={{ height: "100%", overflow: "hidden", borderRadius: 39, background: "linear-gradient(160deg, #0f356f, #081f49 62%, #061832)", color: "#f7fbff" }}>
          <div style={{ width: 94, height: 27, margin: "12px auto 0", borderRadius: 20, background: "#030b18" }} />
          <div style={{ padding: "28px 25px" }}>
            <div style={{ color: "#a8d7ff", fontSize: 16 }}>SwarmDrop</div>
            <div style={{ marginTop: 24, fontSize: 32, fontWeight: 800, letterSpacing: -1 }}>设备列表</div>
            <div style={{ marginTop: 11, color: "#8fbbe9", fontSize: 16 }}>跨网络也能直接发现</div>
            <div style={{ display: "grid", gap: 12, marginTop: 37 }}>
              {["MacBook Pro", "Android 平板", "工作室电脑"].map((name) => (
                <div key={name} style={{ display: "flex", alignItems: "center", gap: 12, padding: "16px 14px", border: "1px solid rgba(156, 212, 255, .18)", borderRadius: 18, background: "rgba(112, 182, 255, .1)" }}>
                  <span style={{ color: "#84e5ac" }}>●</span>
                  <span style={{ fontSize: 16, fontWeight: 700 }}>{name}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
      <svg width="1920" height="1080" style={{ position: "absolute", inset: 0, opacity: clamp(local, 58, 92) * 0.8 }}>
        <path d="M890 550 Q1050 430 1235 520" fill="none" stroke="#9fdcff" strokeDasharray="10 19" strokeWidth="5" />
        {[0.2, 0.54, 0.86].map((offset) => {
          const movement = ((local - 76) / 92 + offset) % 1;
          return <circle key={offset} cx={890 + 345 * Math.max(0, movement)} cy={550 - 28 * Math.sin(movement * Math.PI)} r="5" fill="#d9f3ff" />;
        })}
      </svg>
    </AbsoluteFill>
  );
}

function NetworkIcon() {
  return (
    <svg viewBox="0 0 320 240" width="330" height="250" aria-hidden>
      <circle cx="160" cy="120" r="49" fill="#1c7fe2" opacity=".95" />
      {[[50, 62], [270, 65], [52, 190], [269, 188]].map(([x, y]) => (
        <g key={`${x}-${y}`}>
          <line x1="160" y1="120" x2={x} y2={y} stroke="#9bd7ff" strokeDasharray="8 12" strokeWidth="3" opacity=".8" />
          <rect x={x - 20} y={y - 20} width="40" height="40" rx="12" fill="#164e9c" stroke="#a8ddff" />
        </g>
      ))}
      <circle cx="160" cy="120" r="80" fill="none" stroke="#5ccaff" strokeOpacity=".28" strokeWidth="2" />
    </svg>
  );
}

function LockIcon() {
  return (
    <div style={{ display: "grid", width: 230, height: 250, placeItems: "center", border: "1px solid rgba(132, 231, 172, .38)", borderRadius: 52, background: "linear-gradient(145deg, rgba(39, 167, 115, .32), rgba(10, 73, 63, .35))", boxShadow: "0 0 80px rgba(64, 225, 149, .15)" }}>
      <div style={{ position: "relative", width: 120, height: 104, border: "12px solid #a6f2c4", borderRadius: 28, background: "#1f9d6e" }}>
        <div style={{ position: "absolute", left: 27, top: -87, width: 42, height: 82, border: "12px solid #a6f2c4", borderBottom: 0, borderRadius: "32px 32px 0 0" }} />
        <div style={{ position: "absolute", left: 50, top: 35, width: 19, height: 30, borderRadius: 12, background: "#0a6048" }} />
      </div>
    </div>
  );
}

function McpIcon() {
  return (
    <div style={{ width: 360, padding: 25, border: "1px solid rgba(137, 235, 182, .4)", borderRadius: 28, background: "linear-gradient(145deg, rgba(19, 92, 83, .72), rgba(9, 39, 70, .76))", boxShadow: "0 20px 65px rgba(0, 0, 0, .22)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, color: "#eafff4", fontSize: 23, fontWeight: 800 }}><span style={{ color: "#83e8ae" }}>✦</span> MCP Bridge</div>
      <div style={{ marginTop: 19, padding: "13px 15px", borderRadius: 14, background: "rgba(3, 21, 41, .55)", color: "#99d8ff", fontFamily: "monospace", fontSize: 19 }}>send_files(...)</div>
      <div style={{ marginTop: 16, color: "#aee6cc", fontSize: 16 }}>● 本地已连接 · 127.0.0.1</div>
    </div>
  );
}

function FeatureSlide({ from, kicker, title, description, accent, visual }: { from: number; kicker: string; title: string; description: string; accent: string; visual: React.ReactNode }) {
  const frame = useCurrentFrame();
  const local = frame - from;
  const opacity = clamp(local, 0, 16) * fadeOut(local, 72, 90);

  return (
    <AbsoluteFill style={{ opacity }}>
      <Background opacity={opacity} />
      <div style={{ display: "flex", height: "100%", alignItems: "center", justifyContent: "center", gap: 145, padding: "0 190px" }}>
        <div style={{ width: 720, opacity: clamp(local, 5, 26), translate: `${interpolate(local, [0, 26], [-48, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px 0` }}>
          <div style={{ color: accent, fontSize: 26, fontWeight: 800, letterSpacing: 5 }}>{kicker}</div>
          <div style={{ marginTop: 25, color: "#f6fbff", fontSize: 96, fontWeight: 850, letterSpacing: -5, lineHeight: 1.08 }}>{title}</div>
          <div style={{ marginTop: 30, color: "#b7d2ec", fontSize: 36, lineHeight: 1.45 }}>{description}</div>
        </div>
        <div style={{ opacity: clamp(local, 20, 44), translate: `${interpolate(local, [20, 44], [68, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px 0`, scale: interpolate(local, [20, 44], [0.88, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>{visual}</div>
      </div>
    </AbsoluteFill>
  );
}

export const HeroLoop: React.FC = () => {
  return (
    <AbsoluteFill style={{ overflow: "hidden", fontFamily: "Inter, Arial, 'PingFang SC', sans-serif" }}>
      <NetworkScene />
      <ProductScene />
      <FeatureSlide from={330} kicker="ANY NETWORK" title="跨网络直达" description="局域网、NAT 打洞、中继\n自动选择最快路径" accent="#87ccff" visual={<NetworkIcon />} />
      <FeatureSlide from={420} kicker="PRIVATE BY DESIGN" title="端到端加密" description="不经过中央服务器\n文件只属于收发双方" accent="#8ef0b3" visual={<LockIcon />} />
      <FeatureSlide from={510} kicker="BUILT FOR AGENTS" title="内置 MCP" description="让 AI 直接调度你的设备" accent="#92edbd" visual={<McpIcon />} />
    </AbsoluteFill>
  );
};
