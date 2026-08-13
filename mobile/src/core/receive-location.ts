import { Directory, Paths } from "expo-file-system";
import { useEffect, useMemo, useSyncExternalStore } from "react";
import { AppState, Platform } from "react-native";
import { getMobileCore } from "@/core/mobile-core";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 用户可见的**接收区**——收到的文件落在这里。
 *
 * 与应用私有数据区（`@/core/paths` 的 `getPrivateDataDir()`）是两个角色：那边装库和暂存，
 * 用户看不见也不该看见；这边装用户的文件，用户必须能用系统文件管理器找到它。
 *
 * 平台形态：
 * - **iOS** — 恒为 `Documents/`。`UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace`
 *   让它出现在「文件」App 的 `On My iPhone / SwarmDrop` 下。用户无需选择，也无从选择。
 * - **Android** — 用户经系统目录选择器指定的 SAF tree。`Android/data/` 自 Android 11 起
 *   连系统文件管理器都不可达，所以没有「默认可见位置」可用，必须问一次。
 *
 * **不存在私有目录回退。** 这曾经是 `resolveReceiveLocation(): string` 的必然产物——那个
 * 签名逼着它任何情况下都得返回点什么，于是落到了应用私有目录，收到的文件从此对系统 picker
 * 不可见，既转发不出去也在文件管理器里找不到。三态就是为了让「还没配置」能被说出来。
 */
export type ReceiveLocation =
  | { status: "ready"; uri: string }
  /** Android 尚未选择目录（含首启与用户主动清空）。 */
  | { status: "unconfigured" }
  /** 曾配置但当前不可用：目录被删、授权被撤销、或 provider 故障。带上原路径帮用户定位。 */
  | { status: "revoked"; previousUri: string };

/** Android 需要用户选一次目录；iOS 恒有落点，不暴露选择入口。 */
export function requiresUserChoice(): boolean {
  return Platform.OS === "android";
}

function locationFrom(receivePath: string | null): ReceiveLocation {
  // iOS 恒有落点：`Documents` 由系统提供，用户无从也无需选择。
  if (Platform.OS === "ios")
    return { status: "ready", uri: Paths.document.uri };
  return receivePath
    ? { status: "ready", uri: receivePath }
    : { status: "unconfigured" };
}

/**
 * 上一次探活的结论：`true` = 那个已配置的目录当时访问不了。
 *
 * **为什么要存起来。** 探活是文件系统调用，不能塞进渲染路径；但如果只在「接受入站请求」
 * 那一处探，`revoked` 就只有一个生产者和一个消费者——设置页会把一个已经失效的授权显示成
 * 正常的文件夹名（而那正是用户会去修它的地方），引导判据也看不见它，于是「完成状态不会
 * 与真实状态漂移」只对**未配置**成立，对**已失效**不成立。
 *
 * 存一个布尔而不是整个 `ReceiveLocation`：目录是哪个由配置说了算，这里只记「上次探它没过」。
 */
let lastProbeFailed = false;

/** 探活结论变化时通知订阅者——它不在 zustand 里，得自己广播。 */
const probeListeners = new Set<() => void>();

function setProbeFailed(failed: boolean): void {
  if (lastProbeFailed === failed) return;
  lastProbeFailed = failed;
  for (const listener of probeListeners) listener();
}

/** 把探活结论叠加到配置上。快照版与订阅版共用——数据来源不同，判据只有这一份。 */
function withProbe(
  location: ReceiveLocation,
  probeFailed: boolean,
): ReceiveLocation {
  if (location.status !== "ready" || !probeFailed) return location;
  return { status: "revoked", previousUri: location.uri };
}

/**
 * 当前落点：配置 + 上一次探活的结论。**本函数自身不做文件系统调用。**
 *
 * 用于热路径与拿不到 hook 的地方（接收策略补全、引导判据、事件回调）。要求「此刻确实
 * 可写」的调用点走 {@link probeReceiveLocation}，它会顺带刷新这里读到的结论。
 *
 * 渲染路径请用 {@link useReceiveLocation}——本函数读的是快照，配置变了不会重渲染。
 */
export function getReceiveLocation(): ReceiveLocation {
  return withProbe(
    locationFrom(usePreferencesStore.getState().receivePath),
    lastProbeFailed,
  );
}

/** {@link getReceiveLocation} 的订阅版本。返回值引用稳定，可直接进依赖数组。 */
export function useReceiveLocation(): ReceiveLocation {
  const receivePath = usePreferencesStore((s) => s.receivePath);
  const probeFailed = useSyncExternalStore(
    (onChange) => {
      probeListeners.add(onChange);
      return () => probeListeners.delete(onChange);
    },
    () => lastProbeFailed,
  );
  // 探活结论一变，设置页那一行立刻从文件夹名换成「原接收位置不可用」。
  return useMemo(
    () => withProbe(locationFrom(receivePath), probeFailed),
    [receivePath, probeFailed],
  );
}

/**
 * 把当前落点推给内核，供未显式指定落点的设备策略在**自动接收**时使用。
 *
 * 自动接收不经过 `TransferOfferHost`，因此拿不到这里的三态判定——内核需要自己有一份。
 * 宿主此前的做法是在设置信任策略时把当时的落点抄进每台设备的策略里，那份快照会随用户
 * 换目录而过期：自动接收于是继续往旧目录写，或在接受之后才失败。
 *
 * 节点没起来时内核侧静默忽略，由 `startNode` 之后再推一次兜底。
 */
export async function syncReceiveLocationToCore(): Promise<void> {
  const location = getReceiveLocation();
  if (location.status !== "ready") return;
  try {
    await getMobileCore().setDefaultSaveLocation(location.uri);
  } catch (err) {
    // 推送失败不该影响用户正在做的事：最坏是自动接收退回手动确认，而不是选目录报错。
    console.warn("[receive-location] 同步落点到内核失败:", err);
  }
}

/**
 * 让落点的失效在**用户能看到的地方**被发现，而不是等到下一次传输。
 *
 * 目录是在应用外面被删/被撤权的（系统文件管理器、系统设置），所以回到前台是唯一能捕捉到
 * 这件事的时机。没有它，`revoked` 仍然只在接受入站请求那一刻才产生——设置页照旧显示一个
 * 早已失效的文件夹名，引导判据也照旧认为一切就绪。
 *
 * 挂在根 layout（运行时单例的位置）。Android 才有意义：iOS 的 `Documents` 由系统保证存在。
 */
export function useReceiveLocationWatch(): void {
  useEffect(() => {
    if (!requiresUserChoice()) return;
    probeReceiveLocation();
    const subscription = AppState.addEventListener("change", (next) => {
      if (next === "active") probeReceiveLocation();
    });
    return () => subscription.remove();
  }, []);
}

/**
 * 探活并**记住结论**——接受入站请求前、以及应用回到前台时调用。
 *
 * 用 `Directory.exists`（语义是「存在**且可访问**」）而非 `list()`：后者要枚举整个目录，
 * 用户选 Downloads 这类大目录时代价可观，而我们只想知道能不能访问。
 *
 * **探测抛错也判 `revoked`。** 这与收件箱的 `missing` 标记是反过来的取舍：那边必须区分
 * 「明确不存在」与「查询本身抛错」，因为 missing 是**单向持久标记**，一次 SAF 抖动会永久
 * 锁死一个好文件（见 toolchain.md 的 vivo 实证）。而 `revoked` 每次探活重算、不落库，
 * 误判的代价只是一次多余的提示——相比之下「接受之后才静默写不进去」要糟糕得多。
 */
export function probeReceiveLocation(): ReceiveLocation {
  const configured = locationFrom(usePreferencesStore.getState().receivePath);
  if (configured.status !== "ready") {
    setProbeFailed(false);
    return configured;
  }
  // iOS 的 Documents 由系统保证存在，探活是纯开销。
  if (Platform.OS === "ios") {
    setProbeFailed(false);
    return configured;
  }

  let usable: boolean;
  try {
    usable = new Directory(configured.uri).exists;
  } catch {
    usable = false;
  }
  setProbeFailed(!usable);
  return usable
    ? configured
    : { status: "revoked", previousUri: configured.uri };
}

export type DirectoryPick =
  | { outcome: "picked"; uri: string }
  | { outcome: "cancelled" }
  | { outcome: "unusable"; uri: string };

/**
 * 唤起系统目录选择器并探活，**不持久化**。
 *
 * 全局落点、本次接收的覆盖、按设备的覆盖——三条路都要「选一个目录并确认它能用」，此前
 * 各写各的（连这次把探活从 `list()` 换成 `exists` 都得改三个文件）。判据收在这里一份。
 *
 * **不返回 `ReceiveLocation`**：那个类型里的 `revoked` 带着 `previousUri`，意思是「你原来
 * 用的那个没了」；用它表达「你刚选的这个不能用」会让 UI 把用户**刚挑中**的目录称作「原
 * 位置」。两件事本来就不是一回事，类型也不该合用。
 */
export async function pickDirectory(): Promise<DirectoryPick> {
  let directory: Directory;
  try {
    directory = await Directory.pickDirectoryAsync();
  } catch {
    // 取消走的就是这条 reject。真正的 picker 故障也落这里——代价是失去一条诊断信息，
    // 换来的是不把最常见的那条路径污名化。
    return { outcome: "cancelled" };
  }
  // 用手上这个句柄探活，不要拿 uri 再造一个：那等于顺带断言 `Directory(uri)` 与 picker
  // 返回的 URI 形态逐字节一致，而我们要验的本来就是这个句柄指向的东西。
  return directory.exists
    ? { outcome: "picked", uri: directory.uri }
    : { outcome: "unusable", uri: directory.uri };
}

/**
 * 选定并持久化**全局**接收目录（Android）。
 *
 * expo-file-system 的 `FilePickerContract` 已经替我们调了 `takePersistableUriPermission`，
 * 所以授权跨重启存活，不需要自己写原生代码。
 *
 * 只有 `picked` 才落盘。**不返回旧状态**——那会让「已经有落点、但这次没换成」与「换成功了」
 * 在调用方眼里长得一样。
 */
export async function pickReceiveLocation(): Promise<DirectoryPick> {
  if (!requiresUserChoice()) {
    const current = getReceiveLocation();
    return current.status === "ready"
      ? { outcome: "picked", uri: current.uri }
      : { outcome: "cancelled" };
  }

  const picked = await pickDirectory();
  if (picked.outcome === "picked") {
    usePreferencesStore.getState().setReceivePath(picked.uri);
    // 刚探过、刚选中的目录当然是好的：不清掉旧结论，界面会继续显示「原接收位置不可用」。
    setProbeFailed(false);
    // 自动接收读的是内核里那一份，换了目录必须同步过去——否则它还往旧目录写。
    void syncReceiveLocationToCore();
  }
  return picked;
}
