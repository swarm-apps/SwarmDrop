import { useEffect } from "react";
import { AppState, type AppStateStatus } from "react-native";
import { fireCancelUpdateReady, fireNotifyUpdateReady } from "@/core/notifier";
import { useUpdate } from "@/hooks/use-update";

/**
 * 产物就绪且 app 不在前台时,发一条「新版本已下载,点击安装」。
 *
 * 这是熄屏更新那条路的最后一环:下载在后台完成 → 安装 intent 因 Background Activity
 * Launch 限制发不出去(见 lib/expo-installer 的前台门禁)→ 应用内 UI 用户也看不见。
 * 通知是唯一能触达他的界面,而点击通知回到前台恰好让 useAutoInstall 能合法拉起系统安装框。
 *
 * 前台判断在 notifier 里(与配对 / 传输告警同一策略:前台有应用内 UI 承接,不发系统通知)。
 * 权限走**只检查不请求**的路径 —— 更新通知不值得为它单独弹一次权限请求框。
 */
export function useUpdateReadyNotification(): void {
  const { status, release } = useUpdate();
  const version = release?.version ?? null;

  useEffect(() => {
    if (status !== "ready" || !version) {
      fireCancelUpdateReady();
      return;
    }
    // 就绪那一刻若已在后台,这一发就送达了(notifier 内部有前台判断,前台时是 no-op)。
    fireNotifyUpdateReady(version);
    // **还要监听离开前台**:下载在前台完成、用户取消了系统安装框、然后切走 —— status 与
    // version 都没变,只靠上面那一发的话这条提醒永远不会出现,而产物就那么躺着没人管。
    const sub = AppState.addEventListener("change", (next: AppStateStatus) => {
      if (next !== "active") fireNotifyUpdateReady(version);
    });
    // 离开 ready(装上了 / 产物失效 / 用户 retry)就撤掉这条通知,别留一个点了没反应的入口。
    return () => {
      sub.remove();
      fireCancelUpdateReady();
    };
  }, [status, version]);
}
