/**
 * 文本接收确认的根级宿主。
 *
 * 它与文件 offer、配对请求同级，确保用户不在收件箱时也不会错过需要决策的文本。
 * 队列实体仍由 Rust 端持久化；宿主只保存当前读取到的投影，事件失败后保留旧投影。
 */
import { Trans, useLingui } from "@lingui/react/macro";
import { useCallback, useEffect, useState } from "react";
import { ScrollView } from "react-native";
import {
  MobileCoreEvent_Tags,
  type MobilePendingTextDelivery,
} from "react-native-swarmdrop-core";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Text } from "@/components/ui/text";
import { subscribeCoreEvents } from "@/core/event-bus";
import { getMobileCore } from "@/core/mobile-core";
import { toast } from "@/lib/toast";
import { useInboxStore } from "@/stores/inbox-store";

export function TextDeliveryAttentionHost() {
  const { t } = useLingui();
  const [pendingTexts, setPendingTexts] = useState<MobilePendingTextDelivery[]>(
    [],
  );
  const [responding, setResponding] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setPendingTexts(await getMobileCore().pendingTextDeliveries());
    } catch (error) {
      // 桥接暂时不可用时不能清空当前确认框，否则用户失去重试入口。
      console.warn("[text-delivery-attention] refresh failed:", error);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unsubscribe = subscribeCoreEvents((event) => {
      if (event.tag === MobileCoreEvent_Tags.TextDeliveryAttention)
        void refresh();
    });
    const timer = setInterval(() => void refresh(), 30_000);
    return () => {
      unsubscribe();
      clearInterval(timer);
    };
  }, [refresh]);

  const pending = pendingTexts[0] ?? null;
  const respond = useCallback(
    async (accepted: boolean) => {
      if (!pending || responding) return;
      setResponding(true);
      try {
        await getMobileCore().confirmTextDelivery(pending.deliveryId, accepted);
        setPendingTexts((items) =>
          items.filter((item) => item.deliveryId !== pending.deliveryId),
        );
        if (accepted) await useInboxStore.getState().refresh();
        await refresh();
      } catch (error) {
        toast.error(accepted ? t`接收文本失败` : t`拒绝文本失败`, error);
      } finally {
        setResponding(false);
      }
    },
    [pending, refresh, responding, t],
  );

  if (!pending) return null;

  return (
    <AlertDialog open>
      <AlertDialogContent testID="text-delivery-confirmation">
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Trans>接收文本</Trans>
          </AlertDialogTitle>
          <AlertDialogDescription>
            <Trans>{pending.peerName} 想向你发送一段文本。</Trans>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <ScrollView className="max-h-72 rounded-xl bg-muted/50 p-3">
          <Text selectable className="text-sm leading-6 text-foreground">
            {pending.body}
          </Text>
        </ScrollView>
        <AlertDialogFooter>
          <AlertDialogCancel
            className="flex-1"
            disabled={responding}
            onPress={() => void respond(false)}
            testID="text-delivery-reject-button"
          >
            <Text>
              <Trans>拒绝</Trans>
            </Text>
          </AlertDialogCancel>
          <AlertDialogAction
            className="flex-1"
            disabled={responding}
            onPress={() => void respond(true)}
            testID="text-delivery-accept-button"
          >
            <Text>
              {responding ? <Trans>处理中…</Trans> : <Trans>接收</Trans>}
            </Text>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
