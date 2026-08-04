/**
 * SentInvites
 * 「已发出的邀请」——列出本机未过期的配对邀请并提供撤销（openspec: invite-persistence）。
 *
 * ## 为什么贴着生成屏，而不是收在设置页（2026-08-04 改）
 *
 * 它此前住在设置页，理由写的是「『我有几条邀请在外面飘』是个**管理**问题，而配对屏是一次性
 * 流程屏」。那条推导只看了「一段时间之后想清理」这一种场景，漏掉了更要紧的一种：
 * **刚发错人**。邀请是一次性信任凭证、24 小时有效，「发错了赶紧撤回」发生在生成之后的几秒到
 * 几分钟内，而那一刻用户就站在生成屏上——把撤销藏进设置页等于要求他在最慌的时候多走两跳。
 *
 * 三端同此位置（桌面配对生成屏 / Web 配对面板 / 移动生成邀请屏）。
 *
 * ## 列表里没有邀请链接本身
 *
 * capability 明文不落盘也不出注册表（invite-persistence design D4），重启后拼不回原始链接。
 * 所以这里只能显示元数据 + 撤销；想再分享就重新生成。
 */

import { useCallback, useEffect, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { toast } from "sonner";
import { Link2Off } from "lucide-react";
import { Button } from "@/components/ui/button";
import { commands, type PairInviteListItem } from "@/lib/bindings";
import { getErrorMessage, isErrorKind } from "@/lib/errors";
import { formatTimeLeft } from "@/lib/format";
import { usePairingStore } from "@/stores/pairing-store";

export function SentInvites() {
  const { t } = useLingui();
  const [invites, setInvites] = useState<PairInviteListItem[]>([]);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const generatedAt = usePairingStore((s) => s.activeInvite?.generatedAt ?? null);

  const refresh = useCallback(async () => {
    try {
      setInvites(await commands.listPairInvites());
    } catch (err) {
      // 节点没启动时注册表本就是空的，这不是错误——静默当空列表处理。
      if (!isErrorKind(err, "NodeNotStarted")) {
        toast.error(getErrorMessage(err));
      }
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
  useEffect(() => {
    void refresh();
  }, [refresh, generatedAt]);

  const handleRevoke = useCallback(
    async (id: string) => {
      setRevokingId(id);
      try {
        // 返回值是「有没有落盘」。没落盘时撤销只在本次运行内生效 —— 重启后那条邀请会
        // 复活，必须说出来，否则用户以为已经撤销干净了。
        const persisted = await commands.revokePairInviteById(id);
        if (persisted) {
          toast.success(t`邀请已撤销`);
        } else {
          toast.warning(t`邀请已撤销，但没能保存`, {
            description: t`重启应用后它可能恢复可用，建议稍后再撤销一次。`,
          });
        }
      } catch (err) {
        toast.error(getErrorMessage(err));
      } finally {
        setRevokingId(null);
        void refresh();
      }
    },
    [refresh, t],
  );

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2 px-1">
        <span className="text-sm font-medium text-foreground">
          <Trans>已发出的邀请</Trans>
        </span>
        {invites.length > 0 && (
          <span className="rounded-full bg-foreground/[0.045] px-2 py-0.5 text-[11px] font-semibold tabular-nums text-muted-foreground dark:bg-white/[0.06]">
            {invites.length}
          </span>
        )}
      </div>

      {invites.length === 0 ? (
        <p className="px-1 text-xs leading-5 text-muted-foreground">
          <Trans>
            邀请有效期 24 小时，生成后会出现在这里，随时可以撤销。
          </Trans>
        </p>
      ) : (
        <>
          <ul className="flex flex-col gap-1.5">
            {invites.map((invite) => (
              <li
                key={invite.id}
                className="glass-control flex items-center justify-between gap-3 rounded-[14px] px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="text-xs font-medium text-foreground">
                    {invite.consumed ? (
                      <Trans>已被对方使用</Trans>
                    ) : (
                      <Trans>等待对方使用</Trans>
                    )}
                  </p>
                  <p className="mt-0.5 text-[11px] tabular-nums text-muted-foreground">
                    <RemainingLabel expiresAt={invite.expiresAt} />
                    {/* 只露哈希前 8 位：够用来区分两条邀请，铺满整行没有意义 */}
                    <span className="ml-2 font-mono opacity-70">
                      {invite.id.slice(0, 8)}
                    </span>
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 shrink-0 gap-1.5 px-2.5 text-xs"
                  onClick={() => void handleRevoke(invite.id)}
                  disabled={revokingId === invite.id}
                >
                  <Link2Off className="size-3.5" />
                  <Trans>撤销</Trans>
                </Button>
              </li>
            ))}
          </ul>
          <p className="px-1 text-[11px] leading-4 text-muted-foreground">
            <Trans>
              邀请凭证不会被保存，所以这里只能显示状态。需要再分享请重新生成一条。
            </Trans>
          </p>
        </>
      )}
    </div>
  );
}

/**
 * 剩余有效期文案。
 *
 * 是组件而不是返回 string 的函数：方向词（「…后失效」）要过 Lingui，字符串拼接做不到
 * ——`formatTimeLeft` 只给本地化的时长本身，方向词归这里。
 *
 * 已过期的条目本不该出现（后端按当前时间过滤），但时钟跳变或列表未刷新时会撞上，
 * 显式给一句比显示负数好。
 */
function RemainingLabel({ expiresAt }: { expiresAt: number }) {
  const seconds = expiresAt - Math.floor(Date.now() / 1000);
  if (seconds <= 0) return <Trans>已过期</Trans>;
  return <Trans>{formatTimeLeft(seconds)} 后失效</Trans>;
}
