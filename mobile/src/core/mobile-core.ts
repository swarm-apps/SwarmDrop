import {
  createMobileCore,
  type MobileCorePort,
} from "react-native-swarmdrop-core";
import { mobileEventBus } from "./event-bus";
import { Keychain } from "./keychain";

const hostAdapters = {
  keychain: new Keychain(),
  eventBus: mobileEventBus,
};

let corePromise: Promise<MobileCorePort> | null = null;
let core: MobileCorePort | null = null;

export function initMobileCore(): Promise<MobileCorePort> {
  if (corePromise !== null) {
    return corePromise;
  }

  corePromise = Promise.resolve(
    createMobileCore(hostAdapters.keychain, hostAdapters.eventBus),
  ).then((instance) => {
    core = instance;
    return instance;
  });
  return corePromise;
}

export function getMobileCore(): MobileCorePort {
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
