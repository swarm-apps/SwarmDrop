import { useEffect } from "react";
import {
  cancelAnimation,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withSequence,
  withTiming,
} from "react-native-reanimated";

/**
 * 无限透明度脉冲(闪烁/呼吸)循环:1 → min → 1。返回可直接挂到 Animated.View 的 style。
 * cancel + 复位清理内置在这里 —— 调用方不必各自记 cleanup(曾经两处手写、
 * 一处漏 cancel 导致卸载后动画泄漏在 UI 线程的教训)。
 */
export function usePulseOpacity({
  min,
  duration,
  enabled = true,
}: {
  /** 循环的最低透明度:0 = 硬闪(caret),0.4 = 呼吸(扫描 chip)。 */
  min: number;
  /** 单程时长(ms),一个完整循环是 2×duration。 */
  duration: number;
  enabled?: boolean;
}) {
  const opacity = useSharedValue(1);

  useEffect(() => {
    if (!enabled) return;
    opacity.value = withRepeat(
      withSequence(withTiming(min, { duration }), withTiming(1, { duration })),
      -1,
    );
    return () => {
      cancelAnimation(opacity);
      opacity.value = 1;
    };
  }, [enabled, min, duration, opacity]);

  return useAnimatedStyle(() => ({ opacity: opacity.value }));
}
