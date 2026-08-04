"use client";

// 设备的别名与分组编辑。
//
// **别名与分组是纯本机偏好**——不写进 keychain 的 `PairedDeviceInfo`，也不同步给对端。
// 所以它整条链路都在前端：写 `preferences-store`（localStorage），读也从那里读，
// 不经过 wasm。这也是它能在 Web 端先落地、而信任策略编辑不能的原因（后者要内核写入路径）。
//
// 一个对话框同时管两件事（这台设备的别名 / 这台设备属于哪些组），而**新建分组就地完成**——
// 为了建一个组把用户弹到另一个页面再弹回来，是这类轻量分类最典型的过度设计。

import { Trans, useLingui } from "@lingui/react/macro";
import { Check, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { deviceDisplayName, sortDeviceGroups } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/cn";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";
import type { Device } from "../_lib/view-types";

export function DeviceOrganizationDialog({
  device,
  onOpenChange,
}: {
  /** null = 关闭。用「哪台设备」而不是 open 布尔量驱动，省掉一个必须与它同步的状态。 */
  device: Device | null;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useLingui();
  const organization = usePreferences((s) => s.deviceOrganization);
  // selector 只返回 store 内的稳定引用，派生放这里——`pnpm check:zustand-access` 的规则 B。
  const groups = useMemo(() => sortDeviceGroups(organization.groups), [organization.groups]);

  const [alias, setAlias] = useState("");
  const [selectedGroupIds, setSelectedGroupIds] = useState<string[]>([]);
  const [newGroupName, setNewGroupName] = useState("");

  const peerId = device?.peerId;
  // 每次换设备（或重新打开）都从偏好里重新灌一次草稿，否则会带着上一台设备的别名进来。
  useEffect(() => {
    if (!peerId) return;
    setAlias(organization.aliases[peerId] ?? "");
    setSelectedGroupIds(
      Object.entries(organization.groupDeviceIds)
        .filter(([, ids]) => ids.includes(peerId))
        .map(([groupId]) => groupId),
    );
    setNewGroupName("");
    // 只跟 peerId 走：把 organization 也放进依赖会让「用户正在输入时新建了一个组」把草稿冲掉。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [peerId]);

  if (!device) return null;

  const toggleGroup = (groupId: string) => {
    setSelectedGroupIds((prev) =>
      prev.includes(groupId) ? prev.filter((id) => id !== groupId) : [...prev, groupId],
    );
  };

  const addGroup = () => {
    const id = preferencesActions.createGroup(newGroupName);
    if (!id) return;
    // 新建即选中——用户在这个对话框里建组，意图显然是把当前设备放进去。
    setSelectedGroupIds((prev) => [...prev, id]);
    setNewGroupName("");
  };

  const save = () => {
    preferencesActions.setDeviceAlias(device.peerId, alias);
    preferencesActions.setDeviceGroups(device.peerId, selectedGroupIds);
    onOpenChange(false);
  };

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            <Trans>别名与分组</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>别名与分组只保存在本机，不会同步给对方，也不会改变对方看到的设备名。</Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-foreground">
              <Trans>别名</Trans>
            </span>
            <input
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              // 留空即清除别名，回落到对端设备名——这句话要说出来，否则用户不知道怎么撤销。
              placeholder={t`留空则使用 ${deviceDisplayName(device)}`}
              className="h-11 rounded-md border bg-background px-3 text-sm sm:h-9"
            />
          </label>

          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-foreground">
              <Trans>分组</Trans>
            </span>
            {groups.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                <Trans>还没有分组。在下方新建一个，用来按用途或归属把设备归类。</Trans>
              </p>
            ) : (
              <ul className="flex flex-wrap gap-1.5">
                {groups.map((group) => {
                  const selected = selectedGroupIds.includes(group.id);
                  return (
                    <li key={group.id}>
                      <button
                        type="button"
                        onClick={() => toggleGroup(group.id)}
                        aria-pressed={selected}
                        className={cn(
                          "flex min-h-11 items-center gap-1.5 rounded-full border px-3 text-xs transition-colors sm:min-h-8",
                          selected
                            ? "border-transparent bg-[var(--brand-solid)] text-[var(--brand-ink)]"
                            : "text-muted-foreground hover:bg-accent",
                        )}
                      >
                        {selected && <Check className="size-3" aria-hidden />}
                        {group.name}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}

            <div className="mt-1 flex gap-2">
              <input
                value={newGroupName}
                onChange={(event) => setNewGroupName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key !== "Enter") return;
                  event.preventDefault();
                  addGroup();
                }}
                placeholder={t`新建分组`}
                aria-label={t`新建分组`}
                className="h-11 min-w-0 flex-1 rounded-md border bg-background px-3 text-sm sm:h-9"
              />
              <Button
                type="button"
                variant="secondary"
                onClick={addGroup}
                disabled={!newGroupName.trim()}
                className="shrink-0"
              >
                <Plus className="size-4" aria-hidden />
                <Trans>新建</Trans>
              </Button>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            <Trans>取消</Trans>
          </Button>
          <Button onClick={save}>
            <Trans>保存</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
