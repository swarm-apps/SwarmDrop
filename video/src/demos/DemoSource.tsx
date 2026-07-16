import { OffthreadVideo, useVideoConfig } from "remotion";
import { COMP, stageFor } from "./layout";
import type { DemoTimeline } from "./types";

/**
 * 播放原片，但只显示 contentRect 那块内容区（裁掉 OBS 画布黑边），
 * 缩放居中放到画布上；同时按 sourceTrimMs 裁掉开头的启动等待。
 */
export const DemoSource: React.FC<{ timeline: DemoTimeline; videoSrc: string }> = ({
  timeline,
  videoSrc,
}) => {
  const { width, height } = useVideoConfig();
  const stage = stageFor(timeline, { width, height });
  const { source } = timeline;
  const cr = source.contentRect;
  const vScale = stage.width / cr.width; // 把 contentRect 放大到 stage 宽度
  const trimBefore = Math.round((source.sourceTrimMs / 1000) * COMP.fps);

  return (
    <div
      style={{
        position: "absolute",
        left: stage.x,
        top: stage.y,
        width: stage.width,
        height: stage.height,
        overflow: "hidden",
        borderRadius: 20,
        boxShadow:
          "0 50px 130px rgba(3, 12, 30, .55), 0 0 0 1px rgba(255, 255, 255, .06)",
      }}
    >
      <OffthreadVideo
        src={videoSrc}
        trimBefore={trimBefore}
        muted
        style={{
          position: "absolute",
          width: source.width * vScale,
          height: source.height * vScale,
          left: -cr.x * vScale,
          top: -cr.y * vScale,
        }}
      />
    </div>
  );
};
