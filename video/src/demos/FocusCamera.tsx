import { Easing, useCurrentFrame, useVideoConfig } from "remotion";
import { COMP, type CompSize, type Stage, stageFor, targetCenterCanvas } from "./layout";
import type { DemoTimeline } from "./types";

// 镜头进入 / 退出的缓动窗口（毫秒）。
const LEAD_MS = 220;
const RELEASE_MS = 260;
const ease = Easing.bezier(0.16, 1, 0.3, 1);

type Camera = { scale: number; cx: number; cy: number };

/** 按当前时间求镜头状态：无激活 focus 时为 base（全景、居中、1x）。 */
function cameraAt(
  timeline: DemoTimeline,
  stage: Stage,
  comp: CompSize,
  nowMs: number,
): Camera {
  const base: Camera = { scale: 1, cx: comp.width / 2, cy: comp.height / 2 };
  let cam = base;
  for (const ev of timeline.events) {
    if (ev.type !== "focus" || !ev.target) continue;
    const hold = ev.holdMs ?? 1400;
    const start = ev.atMs;
    const end = ev.atMs + hold;
    if (nowMs < start - LEAD_MS || nowMs > end + RELEASE_MS) continue;

    let w: number;
    if (nowMs < start) w = (nowMs - (start - LEAD_MS)) / LEAD_MS;
    else if (nowMs <= end) w = 1;
    else w = 1 - (nowMs - end) / RELEASE_MS;

    const e = ease(Math.max(0, Math.min(1, w)));
    const c = targetCenterCanvas(stage, ev.target);
    const scale = ev.scale ?? 1.2;
    cam = {
      scale: 1 + (scale - 1) * e,
      cx: base.cx + (c.x - base.cx) * e,
      cy: base.cy + (c.y - base.cy) * e,
    };
  }
  return clampCamera(cam, stage, comp);
}

/**
 * 约束镜头，避免平移露出 app 内容外的背景空洞：
 * app 在该轴上窄于可视范围时居中，否则把平移夹到内容边缘内。
 */
function clampCamera(cam: Camera, stage: Stage, comp: CompSize): Camera {
  const halfW = comp.width / 2 / cam.scale;
  const halfH = comp.height / 2 / cam.scale;
  const left = stage.x;
  const right = stage.x + stage.width;
  const top = stage.y;
  const bottom = stage.y + stage.height;
  const minCx = left + halfW;
  const maxCx = right - halfW;
  const minCy = top + halfH;
  const maxCy = bottom - halfH;
  const cx =
    minCx <= maxCx
      ? Math.min(Math.max(cam.cx, minCx), maxCx)
      : (left + right) / 2;
  const cy =
    minCy <= maxCy
      ? Math.min(Math.max(cam.cy, minCy), maxCy)
      : (top + bottom) / 2;
  return { scale: cam.scale, cx, cy };
}

/**
 * 跟随镜头层：把内部内容（原片 + 点击光圈）整体平移 + 缩放到聚焦目标。
 * 画布尺寸由 composition 决定（16:9 / 9:16 复用同一实现）。
 * 全程由 useCurrentFrame 驱动，禁 CSS 动画（design 文档 §5.2）。
 */
export const FocusCamera: React.FC<{
  timeline: DemoTimeline;
  children: React.ReactNode;
}> = ({ timeline, children }) => {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const comp = { width, height };
  const stage = stageFor(timeline, comp);
  const nowMs = (frame / COMP.fps) * 1000;
  const cam = cameraAt(timeline, stage, comp, nowMs);
  const tx = width / 2 - cam.cx * cam.scale;
  const ty = height / 2 - cam.cy * cam.scale;
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        transform: `translate(${tx}px, ${ty}px) scale(${cam.scale})`,
        transformOrigin: "0 0",
      }}
    >
      {children}
    </div>
  );
};
