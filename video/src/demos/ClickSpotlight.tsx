import { interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { COMP, stageFor, targetCenterCanvas } from "./layout";
import type { DemoTimeline } from "./types";

const VISIBLE_MS = 780;

/**
 * 点击光圈层：在 click 事件的目标中心画鼠标指针 + 扩散波纹。
 * 放在 FocusCamera 内部，随镜头缩放一起放大。坐标为画布坐标。
 */
export const ClickSpotlight: React.FC<{ timeline: DemoTimeline }> = ({ timeline }) => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const stage = stageFor(timeline, { width, height });
  const nowMs = (frame / COMP.fps) * 1000;

  return (
    <>
      {timeline.events.map((ev, i) => {
        if (ev.type !== "click" || !ev.target) return null;
        if (nowMs < ev.atMs - 140 || nowMs > ev.atMs + VISIBLE_MS) return null;
        const c = targetCenterCanvas(stage, ev.target);
        const local = nowMs - ev.atMs;
        const p = Math.max(0, Math.min(1, local / VISIBLE_MS));
        const ring = interpolate(p, [0, 1], [24, 94]);
        const ringOpacity = interpolate(p, [0, 0.25, 1], [0, 0.7, 0]);
        const dot = interpolate(local, [-140, 0, 130], [0.7, 1.18, 1], {
          extrapolateLeft: "clamp",
          extrapolateRight: "clamp",
        });
        return (
          <div key={i}>
            <div
              style={{
                position: "absolute",
                left: c.x - ring / 2,
                top: c.y - ring / 2,
                width: ring,
                height: ring,
                borderRadius: "50%",
                border: "3px solid rgba(125, 192, 255, .92)",
                opacity: ringOpacity,
              }}
            />
            <div
              style={{
                position: "absolute",
                left: c.x - 9,
                top: c.y - 9,
                width: 18,
                height: 18,
                borderRadius: "50%",
                background: "rgba(155, 208, 255, .95)",
                boxShadow: "0 0 18px rgba(90, 170, 255, .9)",
                transform: `scale(${dot})`,
              }}
            />
            <svg
              width="30"
              height="34"
              viewBox="0 0 30 34"
              style={{
                position: "absolute",
                left: c.x + 3,
                top: c.y + 3,
                filter: "drop-shadow(0 3px 6px rgba(0,0,0,.5))",
              }}
            >
              <path
                d="M2 2 L2 26 L9 19 L14 30 L18 28 L13 17 L23 17 Z"
                fill="#fff"
                stroke="#141414"
                strokeWidth="1.5"
                strokeLinejoin="round"
              />
            </svg>
          </div>
        );
      })}
    </>
  );
};
