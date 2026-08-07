import { Directory, Paths } from "expo-file-system";
import {
  initLogging,
  MobileCore,
  type MobileCoreLike,
} from "react-native-swarmdrop-core";
import { mobileEventBus } from "./event-bus";
import { ExpoFileAccess } from "./foreign-file-access";
import { Keychain } from "./keychain";

// SQLite 文件落在 documentDirectory 下；mobile-core 内部会拼上 swarmdrop.db
const dataDir = Paths.document.uri;

// 日志落 cache 而不是 document：它是短期诊断数据，不该进 iCloud 备份，也不该出现在
// 用户可见的文件列表里。代价是系统存储紧张时可能被清理，对诊断用途可以接受。
const logDir = new Directory(Paths.cache, "logs").uri;

const hostAdapters = {
  keychain: new Keychain(),
  eventBus: mobileEventBus,
  fileAccess: new ExpoFileAccess(),
};

let corePromise: Promise<MobileCoreLike> | null = null;
let core: MobileCoreLike | null = null;

export function initMobileCore(): Promise<MobileCoreLike> {
  if (corePromise !== null) {
    return corePromise;
  }
  // 必须先于 MobileCore 构造：在此之前 crates/* 的 tracing 宏是空操作，
  // core 自身的初始化日志（keychain 读取、DB 打开）会一并丢掉。
  // Rust 侧幂等，重复调用无害。
  initLogging(logDir);
  corePromise = Promise.resolve(
    new MobileCore(
      hostAdapters.keychain,
      hostAdapters.eventBus,
      hostAdapters.fileAccess,
      dataDir,
    ),
  ).then((instance) => {
    core = instance;
    return instance;
  });
  return corePromise;
}

export function getMobileCore(): MobileCoreLike {
  if (core === null) {
    throw new Error("MobileCore not initialized; call initMobileCore() first");
  }
  return core;
}

export function getMobileHostAdapters(): typeof hostAdapters {
  return hostAdapters;
}

export function teardownMobileCore(): void {
  core = null;
  corePromise = null;
}
