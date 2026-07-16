import AsyncStorage from "@react-native-async-storage/async-storage";
import type { KeyValueStorage } from "@swarm-hive/sdk";

/**
 * 创建 AsyncStorage 支撑的 KeyValueStorage。get 把 AsyncStorage 的 string | null
 * 直接透传(契约一致);set 落盘。
 */
export function createAsyncStorage(): KeyValueStorage {
  return {
    async get(key: string): Promise<string | null> {
      return AsyncStorage.getItem(key);
    },
    async set(key: string, value: string): Promise<void> {
      await AsyncStorage.setItem(key, value);
    },
  };
}
