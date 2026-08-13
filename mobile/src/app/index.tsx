import { Redirect } from "expo-router";
import { useOnboardingRoute } from "@/core/onboarding-flow";

export default function Index() {
  // 引导完成与否是**派生**的（见 @/core/onboarding-flow）：这里拿到的是第一个未满足
  // 步骤的路由，全满足才为 null。存量用户升级后新增的步骤会自然出现在这里。
  const route = useOnboardingRoute();
  return <Redirect href={(route ?? "/(main)") as never} />;
}
