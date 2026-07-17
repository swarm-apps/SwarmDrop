// 画布 / 坐标换算：把「webview 视口坐标」映射到成片画布坐标（画布尺寸由 composition 决定）。
//
// 舞台(stage) = 原片里 contentRect 那块内容区，按宽高双约束缩放居中放到画布上。
// 事件 target 是视口坐标，经 stage 换算成画布坐标，供镜头与点击光圈定位。

import type { DemoRect, DemoTimeline } from "./types";

/** 16:9 官网默认；9:16 短视频变体。fps 两者一致。 */
export const COMP = { width: 1920, height: 1080, fps: 30 } as const;
export const VERTICAL = { width: 1080, height: 1920, fps: 30 } as const;

/** app 内容占画布的宽 / 高比例上限（取更紧的一个，其余为背景留边）。 */
const FIT_W = 0.96;
const FIT_H = 0.88;

export type CompSize = { width: number; height: number };

export type Stage = {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
  viewport: { width: number; height: number };
};

export function stageFor(timeline: DemoTimeline, comp: CompSize): Stage {
  const vp = timeline.viewport ?? {
    width: timeline.source.contentRect.width,
    height: timeline.source.contentRect.height,
  };
  const scale = Math.min(
    (comp.width * FIT_W) / vp.width,
    (comp.height * FIT_H) / vp.height,
  );
  const width = vp.width * scale;
  const height = vp.height * scale;
  return {
    x: (comp.width - width) / 2,
    y: (comp.height - height) / 2,
    width,
    height,
    scale,
    viewport: vp,
  };
}

/** 视口坐标 → 画布坐标（base，未叠加镜头缩放）。 */
export function toCanvas(stage: Stage, vx: number, vy: number) {
  return { x: stage.x + vx * stage.scale, y: stage.y + vy * stage.scale };
}

/** 事件 target（视口坐标，clamp 到视口内）→ 画布中心点。 */
export function targetCenterCanvas(stage: Stage, target: DemoRect) {
  const x = Math.max(0, target.x);
  const y = Math.max(0, target.y);
  const w = Math.min(target.width, stage.viewport.width - x);
  const h = Math.min(target.height, stage.viewport.height - y);
  return toCanvas(stage, x + w / 2, y + h / 2);
}

export const msToFrames = (ms: number) => (ms / 1000) * COMP.fps;

/** 成片时长：最后一个事件结束 + 收尾留白。 */
export function timelineDurationFrames(timeline: DemoTimeline): number {
  const end = timeline.events.reduce((max, ev) => {
    const dur = ev.holdMs ?? (ev.type === "click" ? 900 : 1200);
    return Math.max(max, ev.atMs + dur);
  }, 0);
  const tailMs = 1500;
  return Math.ceil(msToFrames(end + tailMs));
}
