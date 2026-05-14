import {
  MobileCore,
  type MobileCoreLike,
} from "react-native-swarmdrop-core";
import { mobileEventBus } from "./event-bus";
import { Keychain } from "./keychain";

const hostAdapters = {
  keychain: new Keychain(),
  eventBus: mobileEventBus,
};

let corePromise: Promise<MobileCoreLike> | null = null;
let core: MobileCoreLike | null = null;

export function initMobileCore(): Promise<MobileCoreLike> {
  if (corePromise !== null) {
    return corePromise;
  }

  corePromise = Promise.resolve(
    new MobileCore(hostAdapters.keychain, hostAdapters.eventBus),
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
