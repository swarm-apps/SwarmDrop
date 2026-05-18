import { Paths } from "expo-file-system";
import { MobileCore, type MobileCoreLike } from "react-native-swarmdrop-core";
import { mobileEventBus } from "./event-bus";
import { ExpoFileAccess } from "./foreign-file-access";
import { Keychain } from "./keychain";

// SQLite 文件落在 documentDirectory 下；mobile-core 内部会拼上 swarmdrop.db
const dataDir = Paths.document.uri;

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
