import AsyncStorage from "@react-native-async-storage/async-storage";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

interface OnboardingState {
  hasOnboarded: boolean;
  currentStep: number;
}

interface OnboardingActions {
  nextStep(): void;
  prevStep(): void;
  markCompleted(): void;
  reset(): void;
}

export const useOnboardingStore = create<OnboardingState & OnboardingActions>()(
  persist(
    (set) => ({
      hasOnboarded: false,
      currentStep: 0,
      nextStep: () => set((s) => ({ currentStep: s.currentStep + 1 })),
      prevStep: () =>
        set((s) => ({ currentStep: Math.max(0, s.currentStep - 1) })),
      markCompleted: () => set({ hasOnboarded: true }),
      reset: () => set({ hasOnboarded: false, currentStep: 0 }),
    }),
    {
      name: "swarmdrop-onboarding",
      storage: createJSONStorage(() => AsyncStorage),
      partialize: (s) => ({ hasOnboarded: s.hasOnboarded }),
    },
  ),
);

export const waitForOnboardingHydration = (): Promise<void> =>
  new Promise((resolve) => {
    if (useOnboardingStore.persist.hasHydrated()) {
      resolve();
      return;
    }
    const unsub = useOnboardingStore.persist.onFinishHydration(() => {
      unsub();
      resolve();
    });
  });
