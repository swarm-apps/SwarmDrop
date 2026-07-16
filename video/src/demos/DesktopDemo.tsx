import { AbsoluteFill, staticFile } from "remotion";
import { ClickSpotlight } from "./ClickSpotlight";
import demoTimeline from "./data/desktop-home.timeline.json";
import { DemoCaption } from "./DemoCaption";
import { DemoSource } from "./DemoSource";
import { FocusCamera } from "./FocusCamera";
import type { DemoTimeline } from "./types";

export const desktopHomeTimeline = demoTimeline as unknown as DemoTimeline;

/**
 * 16:9 桌面 demo 成片：消费「原片 + 事件时间线」。
 * 分层：背景 → [镜头层: 原片 + 点击光圈] → 字幕层（不随镜头缩放）。
 */
export const DesktopDemo: React.FC = () => {
  const videoSrc = staticFile("demos/desktop-home.mp4");
  return (
    <AbsoluteFill style={{ fontFamily: "Inter, 'PingFang SC', sans-serif" }}>
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(circle at 50% 38%, #12305f 0%, #091c3c 46%, #05101f 100%)",
        }}
      />
      <FocusCamera timeline={desktopHomeTimeline}>
        <DemoSource timeline={desktopHomeTimeline} videoSrc={videoSrc} />
        <ClickSpotlight timeline={desktopHomeTimeline} />
      </FocusCamera>
      <DemoCaption timeline={desktopHomeTimeline} />
    </AbsoluteFill>
  );
};
