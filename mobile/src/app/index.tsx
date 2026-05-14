import { Redirect } from "expo-router";
import { useOnboardingStore } from "@/stores/onboarding-store";

export default function Index() {
  const hasOnboarded = useOnboardingStore((s) => s.hasOnboarded);
  return (
    <Redirect
      href={(hasOnboarded ? "/(main)" : "/onboarding/welcome") as never}
    />
  );
}
