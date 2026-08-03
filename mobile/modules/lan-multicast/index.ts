import { requireOptionalNativeModule } from "expo-modules-core";

interface LanMulticastNativeModule {
  acquire(): void;
  release(): void;
}

/**
 * Android-only。**模块缺席时静默 no-op 而不是抛错**（与 `content-share` 相反）：
 * 这里 iOS 上本来就没有原生实现——iOS 不需要组播锁，系统不做那层过滤，
 * 它挡 mDNS 用的是本地网络权限（`app.json` 的 `NSLocalNetworkUsageDescription`）。
 * 拿不到锁只是回落到「可能收不到组播」，节点照常经 relay 工作，不该中断启动。
 */
const LanMulticast =
  requireOptionalNativeModule<LanMulticastNativeModule>("LanMulticast");

/** 持锁，让 mDNS 收得到组播帧。幂等（原生侧 setReferenceCounted(false)）。 */
export function acquireMulticastLock(): void {
  LanMulticast?.acquire();
}

/** 放锁。持着它 Wi-Fi 芯片不进省电态，节点停了就该放。幂等。 */
export function releaseMulticastLock(): void {
  LanMulticast?.release();
}
