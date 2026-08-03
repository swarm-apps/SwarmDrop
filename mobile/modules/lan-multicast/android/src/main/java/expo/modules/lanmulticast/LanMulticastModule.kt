package expo.modules.lanmulticast

import android.content.Context
import android.net.wifi.WifiManager
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * Android MulticastLock —— 让 mDNS 真的收得到组播包。
 *
 * 不持这把锁时，Wi-Fi 芯片在省电态会把「目的 MAC 不是本机」的组播/广播帧直接丢在
 * 驱动层，应用连 recvfrom 都等不到。于是 libp2p 的 mDNS（`_p2p._udp.local`，
 * 224.0.0.251:5353）既发不出去也收不回来，同一个 Wi-Fi 下的两台设备只能靠公网
 * relay 互相看见——传输全程绕公网，慢且费流量。
 *
 * 锁的生命周期与节点绑定（node running ⇔ lock held），与前台服务同一套心智，
 * 调用点在 `mobile/src/core/lan-multicast.ts`。
 *
 * 需要 manifest 里的 `CHANGE_WIFI_MULTICAST_STATE`（normal 权限，声明即授予，
 * 无运行时弹窗）；由 `plugins/with-android-multicast.js` 注入。
 */
class LanMulticastModule : Module() {
  private var lock: WifiManager.MulticastLock? = null

  override fun definition() = ModuleDefinition {
    Name("LanMulticast")

    Function("acquire") {
      if (lock?.isHeld == true) return@Function
      // 必须用 applicationContext：MulticastLock 的生命周期跨 Activity，
      // 拿 Activity context 建锁会在旋转屏时泄漏那个 Activity。
      val wifi = appContext.reactContext
        ?.applicationContext
        ?.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        ?: return@Function
      lock = wifi.createMulticastLock(LOCK_TAG).apply {
        // 非引用计数：acquire/release 变成幂等的开关，与 JS 侧「节点启停各调一次」
        // 的用法对齐。计数模式下漏掉一次 release 就永远放不掉锁（持续耗电）。
        setReferenceCounted(false)
        acquire()
      }
    }

    Function("release") {
      lock?.takeIf { it.isHeld }?.release()
      lock = null
    }

    // app 被回收时兜底放锁——持着它 Wi-Fi 芯片不会进省电态，明显更耗电。
    OnDestroy {
      lock?.takeIf { it.isHeld }?.release()
      lock = null
    }
  }

  private companion object {
    const val LOCK_TAG = "swarmdrop-mdns"
  }
}
