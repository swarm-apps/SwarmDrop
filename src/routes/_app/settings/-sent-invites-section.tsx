/**
 * SentInvitesSection
 * 设置页「已发出的邀请」区域 — 列出本机未过期的配对邀请并提供撤销（openspec: invite-persistence）
 *
 * 为什么在设置页而不是配对生成屏：邀请活 24 小时且跨重启存活，「我有几条邀请在外面飘」是个
 * **管理**问题，而配对屏是一次性流程屏，把管理清单挤进去会让那条流程变长。
 *
 * 列表里**没有邀请链接本身** —— capability 明文不落盘也不出注册表（invite-persistence
 * design D4），重启后拼不回原始链接。所以这里只能显示元数据 + 撤销；想再分享就重新生成。
 */

import { useCallback, useEffect, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { toast } from "sonner";
import { Link2Off, Ticket } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { commands, type PairInviteListItem } from "@/lib/bindings";
import { getErrorMessage, isErrorKind } from "@/lib/errors";
import { formatTimeLeft } from "@/lib/format";
import { SettingsCard, SettingsRow, SettingsSection } from "./-settings-primitives";

export function SentInvitesSection() {
  const { t } = useLingui();
  const [invites, setInvites] = useState<PairInviteListItem[]>([]);
  const [revokingId, setRevokingId] = useState<string | null>(null);

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

  useEffect(() => {
    void refresh();
  }, [refresh]);

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
    <SettingsSection
      title={<Trans>已发出的邀请</Trans>}
      icon={Ticket}
      aside={
        invites.length > 0 ? (
          <Badge variant="outline">{invites.length}</Badge>
        ) : null
      }
    >
      <SettingsCard>
        {invites.length === 0 ? (
          <SettingsRow
            title={<Trans>没有生效中的邀请</Trans>}
            description={
              <Trans>
                邀请有效期 24 小时，生成后会出现在这里，随时可以撤销。
              </Trans>
            }
          />
        ) : (
          <>
            {invites.map((invite) => (
              <SettingsRow
                key={invite.id}
                title={
                  invite.consumed ? (
                    <Trans>已被对方使用</Trans>
                  ) : (
                    <Trans>等待对方使用</Trans>
                  )
                }
                description={
                  <span className="tabular-nums">
                    <RemainingLabel expiresAt={invite.expiresAt} />
                    {/* 只露哈希前 8 位：够用来区分两条邀请，铺满整行没有意义 */}
                    <span className="ml-2 font-mono text-[11px] opacity-70">
                      {invite.id.slice(0, 8)}
                    </span>
                  </span>
                }
                action={
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleRevoke(invite.id)}
                    disabled={revokingId === invite.id}
                  >
                    <Link2Off className="size-4" />
                    <Trans>撤销</Trans>
                  </Button>
                }
              />
            ))}
            <SettingsRow
              title={<Trans>看不到邀请链接？</Trans>}
              description={
                <Trans>
                  邀请凭证不会被保存，所以这里只能显示状态。需要再分享请重新生成一条。
                </Trans>
              }
            />
          </>
        )}
      </SettingsCard>
    </SettingsSection>
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
