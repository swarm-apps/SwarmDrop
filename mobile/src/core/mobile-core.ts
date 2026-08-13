import { Directory, Paths } from "expo-file-system";
import {
  initLogging,
  MobileCore,
  type MobileCoreLike,
} from "react-native-swarmdrop-core";
import { mobileEventBus } from "./event-bus";
import { ExpoFileAccess } from "./foreign-file-access";
import { Keychain } from "./keychain";
import { cleanupLegacyIosDocuments } from "./legacy-cleanup";
import { getPrivateDataDir } from "./paths";

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
  // SQLite 与接收暂存都挂在这个私有数据区下（mobile-core 内部拼 swarmdrop.db 与 staging/）。
  // **不是** documentDirectory：iOS 的 Documents 已随文件共享整个对用户可见，库和暂存半成品
  // 不能留在那儿。平台差异收在 getPrivateDataDir()。
  //
  // **必须在函数体内取，不能放模块顶层**：iOS 上原生模块没链接时它会抛，而模块顶层的抛错
  // 发生在 `_layout` import 本模块的过程中——那既在 `runBoot` 的 try/catch 之外，也在
  // `<Try catch={ErrorBoundary}>` 之外，用户得到的是致命红屏而不是那张带「重试」的启动
  // 错误屏。放进来它就是一次普通的、可诊断也可重试的启动失败。
  const dataDir = getPrivateDataDir();
  // 私有数据区搬家后，旧位置（iOS 的 `Documents`）里的库与暂存必须清掉——那个目录
  // 现在对用户可见了。放在这里而不是并行的 boot 步骤里：它必须发生在 core 打开新库
  // **之前**，否则不同版本残留的判断会变得依赖时序。
  cleanupLegacyIosDocuments();
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
