// Demo 事件时间线类型（与 e2e/desktop 产出的 timelines/*.json schema 对齐）。

export type DemoRect = { x: number; y: number; width: number; height: number };

export type DemoEvent = {
  /** 相对 `go`（录制放行时刻）的毫秒；也 = 成片中相对 sourceTrim 后起点的时间。 */
  atMs: number;
  type: "caption" | "click" | "focus";
  text?: string;
  caption?: string;
  /** webview 视口坐标；后期经 contentRect / stage 换算成画布坐标。 */
  target?: DemoRect;
  scale?: number;
  holdMs?: number;
};

export type DemoTimeline = {
  schemaVersion: number;
  demo: string;
  source: {
    path: string | null;
    /** 从原片开头裁掉的毫秒（= go 之前的启动等待）。 */
    sourceTrimMs: number;
    width: number;
    height: number;
    /** 内容区（webview 视口）在原片中的像素矩形。 */
    contentRect: DemoRect;
    durationMs?: number;
  };
  viewport: { width: number; height: number } | null;
  events: DemoEvent[];
};
