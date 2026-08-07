/**
 * SentInvites
 * 「已发出的邀请」——列出本机未过期的配对邀请并提供撤销。
 *
 * ## 位置：贴着生成入口，三端一致
 *
 * 邀请是一次性信任凭证、24 小时有效，而它最需要被撤回的时刻是**刚发错人**——那一刻用户就
 * 站在展示邀请的这一屏上。桌面此前把它收在设置页（理由是「有几条在外面飘」属于管理问题），
 * 那条推导漏掉了这个高峰场景，已一并改到配对生成屏。
 *
 * 移动端此前**完全没有这个入口**：uniffi 侧 `listPairInvites` / `revokePairInviteById`
 * 早就导出了，但前端零调用——手机上发出去的邀请在 24 小时内无法撤回，而手机恰恰是最容易
 * 随手发错人的那一端。
 *
 * ## 列表里没有邀请链接本身
 *
 * capability 明文不落盘也不出注册表，重启后拼不回原始链接。所以这里只能显示元数据 + 撤销；
 * 想再分享就重新生成。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { formatTimeLeft } from "@swarmdrop/shared-view";
import { Link2Off } from "lucide-react-native";
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import type { MobileInviteListItem } from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";
import { i18n } from "@/i18n/lingui";
import { toast } from "@/lib/toast";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";

export function SentInvites() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const [invites, setInvites] = useState<MobileInviteListItem[]>([]);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const generatedAt = usePairingInviteStore(
    (s) => s.activeInvite?.generatedAt ?? null,
  );

  const refresh = useCallback(async () => {
    try {
      setInvites(await getMobileCore().listPairInvites());
    } catch (err) {
      // 节点没启动时注册表本就是空的，退化成空列表是对的——但**不能连日志都不留**。
      // 这个 catch 罩着的是「读注册表」这一整件事，读失败与「真的没有邀请」在界面上
      // 长得一模一样（都是那句「生成后会出现在这里」）。真出问题时，用户看到的是一条
      // 安静的谎：邀请明明发出去了，列表却说没有，而撤销入口正好在这里。
      console.warn("[pairing] 读取已发出的邀请失败，本次按空列表展示", err);
      setInvites([]);
    }
  }, []);

  /**
   * 生成一条新邀请后自动刷新。
   *
   * **这是本组件搬到配对屏之后唯一的存在理由**：它服务的场景是「刚发错人，赶紧撤回」，
   * 而那一刻用户刚点完「生成邀请」。只在 mount 时读一次的话，列表里恰好缺的就是刚生成的
   * 那条——用户看到的是一份「不含我刚做的那件事」的清单，撤销入口等于没有。
   *
   * 依赖取 `generatedAt`（毫秒时间戳）而不是 `activeInvite` 对象：原始值，不触发
   * selector 派生新对象那条禁令，也让「重新生成同一串」（不可能，但不必依赖这个前提）
   * 照样能触发。
   */
  // biome-ignore lint/correctness/useExhaustiveDependencies: generatedAt 仅作为重读触发器。
  useEffect(() => {
    void refresh();
  }, [refresh, generatedAt]);

  const handleRevoke = useCallback(
    async (id: string) => {
      setRevokingId(id);
      try {
        // 返回值是「有没有落盘」。没落盘时撤销只在本次运行内生效——重启后那条邀请会复活，
        // 必须说出来，否则用户以为已经撤销干净了，而外面还飘着一条能用的凭证。
        const persisted = await getMobileCore().revokePairInviteById(id);
        if (persisted) {
          toast.success(t`邀请已撤销`);
        } else {
          // 走 error 而不是 success：这不是一次干净的成功，而用户需要知道要再做一次。
          // （本端 toast 只有 done / none / error 三档，没有 warning。）
          toast.error(t`邀请已撤销，但没能保存`, {
            description: t`重启应用后它可能恢复可用，建议稍后再撤销一次。`,
          });
        }
      } catch (err) {
        toast.error(t`撤销失败`, err);
      } finally {
        setRevokingId(null);
        void refresh();
      }
    },
    [refresh, t],
  );

  return (
    <View className="gap-2">
      <View className="flex-row items-center justify-between gap-2">
        <Text className="text-[13px] font-semibold text-foreground">
          <Trans>已发出的邀请</Trans>
        </Text>
        {invites.length > 0 && (
          <Text className="text-[11px] font-semibold tabular-nums text-muted-foreground">
            {invites.length}
          </Text>
        )}
      </View>

      {invites.length === 0 ? (
        <Text className="text-[12px] leading-5 text-muted-foreground">
          <Trans>邀请有效期 24 小时，生成后会出现在这里，随时可以撤销。</Trans>
        </Text>
      ) : (
        <>
          {invites.map((invite) => (
            <View
              key={invite.id}
              className="flex-row items-center justify-between gap-3 rounded-xl border border-border bg-card px-3 py-2"
            >
              <View className="min-w-0 flex-1">
                <Text className="text-[13px] font-medium text-foreground">
                  {invite.consumed ? (
                    <Trans>已被对方使用</Trans>
                  ) : (
                    <Trans>等待对方使用</Trans>
                  )}
                </Text>
                <Text className="mt-0.5 text-[11px] tabular-nums text-muted-foreground">
                  <RemainingLabel expiresAt={Number(invite.expiresAt)} />
                  {/* 只露哈希前 8 位：够用来区分两条邀请，铺满整行没有意义 */}
                  <Text className="font-mono opacity-70">
                    {"  "}
                    {invite.id.slice(0, 8)}
                  </Text>
                </Text>
              </View>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={t`撤销这条邀请`}
                disabled={revokingId === invite.id}
                onPress={() => void handleRevoke(invite.id)}
                className="min-h-11 shrink-0 flex-row items-center gap-1.5 px-2 active:opacity-70 disabled:opacity-50"
              >
                {revokingId === invite.id ? (
                  <ActivityIndicator
                    size="small"
                    color={colors.mutedForeground}
                  />
                ) : (
                  <Link2Off color={colors.mutedForeground} size={14} />
                )}
                <Text className="text-[13px] font-medium text-muted-foreground">
                  <Trans>撤销</Trans>
                </Text>
              </Pressable>
            </View>
          ))}
          <Text className="text-[11px] leading-4 text-muted-foreground">
            <Trans>
              邀请凭证不会被保存，所以这里只能显示状态。需要再分享请重新生成一条。
            </Trans>
          </Text>
        </>
      )}
    </View>
  );
}

/**
 * 剩余有效期文案。
 *
 * 是组件而不是返回 string 的函数：方向词（「…后失效」）要过 Lingui，字符串拼接做不到
 * ——`formatTimeLeft` 只给本地化的时长本身，方向词归这里。桌面同款。
 *
 * **渲染时算一次，不做每秒 tick**：TTL 是 24 小时，列表里显示的是「几小时后失效」，
 * 秒级刷新既无信息量、又要给一整列都上定时器。真正需要秒级跳动的是当面配对时的那个
 * 码面倒计时，那条在 `invite-exchange.tsx` 里另有 `useExpiresCountdown`。
 *
 * 已过期的条目本不该出现（后端按当前时间过滤），但时钟跳变或列表未刷新时会撞上，
 * 显式给一句比显示负数好。
 */
function RemainingLabel({ expiresAt }: { expiresAt: number }) {
  const seconds = expiresAt - Math.floor(Date.now() / 1000);
  if (seconds <= 0) return <Trans>已过期</Trans>;
  return <Trans>{formatTimeLeft(seconds, i18n.locale)} 后失效</Trans>;
}
