import { Easing, interpolate, useCurrentFrame } from "remotion";
import { COMP } from "./layout";
import type { DemoTimeline } from "./types";

/**
 * 字幕层：放在画布安全区（底部居中），不随镜头缩放。
 * 每句字幕从其事件 atMs 显示到下一句字幕出现，淡入淡出由帧驱动。
 */
export const DemoCaption: React.FC<{ timeline: DemoTimeline }> = ({ timeline }) => {
  const frame = useCurrentFrame();
  const nowMs = (frame / COMP.fps) * 1000;

  const caps = timeline.events
    .filter((e) => e.text || e.caption)
    .map((e) => ({ text: (e.text ?? e.caption) as string, at: e.atMs }));
  const spans = caps.map((c, i) => ({
    text: c.text,
    start: c.at,
    end: caps[i + 1]?.at ?? c.at + 3000,
  }));

  const active = spans.find((s) => nowMs >= s.start - 260 && nowMs < s.end);
  if (!active) return null;

  const fadeIn = interpolate(nowMs - active.start, [-260, 140], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const fadeOut = interpolate(active.end - nowMs, [0, 320], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const opacity = Math.min(fadeIn, fadeOut);
  const rise = interpolate(nowMs - active.start, [-260, 140], [18, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.out(Easing.cubic),
  });

  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        bottom: 92,
        display: "flex",
        justifyContent: "center",
        opacity,
      }}
    >
      <div
        style={{
          transform: `translateY(${rise}px)`,
          padding: "16px 32px",
          borderRadius: 16,
          background: "rgba(8, 18, 38, .82)",
          border: "1px solid rgba(140, 190, 255, .18)",
          color: "#eaf3ff",
          fontSize: 38,
          fontWeight: 650,
          letterSpacing: -0.3,
          maxWidth: 1440,
          textAlign: "center",
          boxShadow: "0 20px 60px rgba(0, 0, 0, .4)",
        }}
      >
        {active.text}
      </div>
    </div>
  );
};
