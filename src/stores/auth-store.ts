/**
 * Auth Store
 * 默认启动路径不再要求应用密码；设备身份由 host keychain 初始化。
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { rehydrateSecretStore } from "@/stores/secret-store";
import {
  checkStatus,
  setData,
  getData,
  removeData,
  BiometryType,
  type Status,
} from "@choochmeque/tauri-plugin-biometry-api";

const BIOMETRY_DOMAIN = "com.yexiyue.swarmdrop";
const LOCAL_LOCK_KEY = "local_lock_enabled";

export { BiometryType };

export type LoadingMessageType =
  | "initializing_storage"
  | "generating_keypair"
  | "decrypting_data"
  | "loading_keypair";

export type ErrorMessageType =
  | "password_not_found"
  | "wrong_password"
  | "biometric_not_enabled"
  | "biometric_not_available"
  | "stored_password_not_found";

interface AuthState {
  isSetupComplete: boolean;
  biometricEnabled: boolean;
  biometricAvailable: boolean;
  biometricType: BiometryType;
  isUnlocked: boolean;
  isLoading: boolean;
  loadingMessage: LoadingMessageType | null;
  error: ErrorMessageType | string | null;
  _tempPassword: string | null;

  checkBiometricAvailability: () => Promise<void>;
  checkSetupStatus: () => void;
  setupPassword: (password: string) => Promise<void>;
  enableBiometric: (password?: string) => Promise<void>;
  disableBiometric: () => Promise<void>;
  clearTempPassword: () => void;
  unlock: (password: string) => Promise<boolean>;
  unlockWithBiometric: () => Promise<boolean>;
  lock: () => void;
  clearError: () => void;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      isSetupComplete: true,
      biometricEnabled: false,
      biometricAvailable: false,
      biometricType: BiometryType.None,
      isUnlocked: true,
      isLoading: false,
      loadingMessage: null,
      error: null,
      _tempPassword: null,

      checkSetupStatus: () => {
        set({ isSetupComplete: true, isUnlocked: true, _tempPassword: null });
      },

      async checkBiometricAvailability() {
        try {
          const status: Status = await checkStatus();
          set({
            biometricAvailable: status.isAvailable,
            biometricType: status.biometryType ?? BiometryType.None,
          });
        } catch (err) {
          console.error("Failed to check biometric status:", err);
          set({
            biometricAvailable: false,
            biometricType: BiometryType.None,
          });
        }
      },

      async setupPassword(_password: string) {
        set({ isLoading: true, loadingMessage: "initializing_storage", error: null });
        try {
          await rehydrateSecretStore();
          set({
            isSetupComplete: true,
            isUnlocked: true,
            isLoading: false,
            loadingMessage: null,
            _tempPassword: null,
          });
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            isLoading: false,
            loadingMessage: null,
          });
          throw err;
        }
      },

      async enableBiometric(_password?: string) {
        set({ isLoading: true, error: null });

        try {
          await setData({
            domain: BIOMETRY_DOMAIN,
            name: LOCAL_LOCK_KEY,
            data: "enabled",
          });
          set({
            biometricEnabled: true,
            isLoading: false,
            _tempPassword: null,
          });
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            isLoading: false,
          });
          throw err;
        }
      },

      async disableBiometric() {
        set({ isLoading: true, error: null });

        try {
          await removeData({
            domain: BIOMETRY_DOMAIN,
            name: LOCAL_LOCK_KEY,
          });
          set({ biometricEnabled: false, isLoading: false });
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            isLoading: false,
          });
          throw err;
        }
      },

      async unlock(_password: string) {
        set({ isLoading: true, loadingMessage: "loading_keypair", error: null });

        try {
          await rehydrateSecretStore();
          set({ isUnlocked: true, isLoading: false, loadingMessage: null });
          return true;
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            isLoading: false,
            loadingMessage: null,
          });
          return false;
        }
      },

      async unlockWithBiometric() {
        const { biometricEnabled, biometricAvailable } = get();
        if (!biometricEnabled) {
          set({ error: "biometric_not_enabled" });
          return false;
        }

        if (!biometricAvailable) {
          set({ error: "biometric_not_available" });
          return false;
        }

        set({ isLoading: true, error: null });

        try {
          const response = await getData({
            domain: BIOMETRY_DOMAIN,
            name: LOCAL_LOCK_KEY,
            reason: "验证身份以解锁 SwarmDrop",
            cancelTitle: "取消",
          });

          if (!response.data) {
            throw new Error("stored_password_not_found");
          }

          await rehydrateSecretStore();
          set({ isUnlocked: true, isLoading: false });
          return true;
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            isLoading: false,
          });
          return false;
        }
      },

      lock() {
        set({ isUnlocked: false });
      },

      clearError() {
        set({ error: null, loadingMessage: null });
      },

      clearTempPassword() {
        set({ _tempPassword: null });
      },
    }),
    {
      name: "auth-store",
      partialize: (state) => ({
        biometricEnabled: state.biometricEnabled,
      }),
      onRehydrateStorage: () => (state) => {
        state?.checkSetupStatus();
      },
    },
  ),
);
