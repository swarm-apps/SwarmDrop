import { Composition } from "remotion";
import { HeroLoop } from "./compositions/HeroLoop";

export const HeroLoopComposition: React.FC = () => {
  return (
    <Composition
      id="HeroLoop"
      component={HeroLoop}
      durationInFrames={600}
      fps={30}
      width={1920}
      height={1080}
    />
  );
};
