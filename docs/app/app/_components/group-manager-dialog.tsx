"use client";

// 分组管理：重命名 / 删除 / 新建。
//
// ## 为什么必须有它
//
// `preferencesActions.renameGroup` 与 `deleteGroup` 早就写好了，但**全仓零调用点**——
// 唯一碰分组的地方是设备卡的「别名与分组」对话框，而那里只能 `createGroup`。于是分组
// **只进不出**：名字打错了改不了，建多了删不掉，唯一的补救是清 localStorage（连别名
// 一起清掉）。有创建无销毁是状态齐全性最硬的一档缺失。
//
// ## 删除要确认，重命名不用
//
// 删组会连带丢掉「哪些设备属于它」这份关系（`deleteGroup` 一并清 `groupDeviceIds`），
// 且不可撤销；重命名随时可以改回来。**代价不对称的动作才配一次确认**——给每个动作都套
// 确认框只会让用户学会闭眼点「确定」。
//
// ## 分组是**本机偏好**，不是内核概念
//
// 全部落 localStorage，内核不知道它们存在。所以这里没有任何 async、没有失败路径，
// 也不需要 toast——改完立刻生效且必然成功。

import { Trans, useLingui } from "@lingui/react/macro";
import { Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { sortDeviceGroups } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";

export function GroupManagerDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useLingui();
  const organization = usePreferences((s) => s.deviceOrganization);

  // selector 只返回 store 内的稳定引用，派生放这里（`check:zustand-access` 规则 B）。
  const groups = useMemo(
    () => sortDeviceGroups(organization.groups),
    [organization.groups],
  );
  const memberCount = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const group of organization.groups) {
      counts[group.id] = organization.groupDeviceIds[group.id]?.length ?? 0;
    }
    return counts;
  }, [organization.groups, organization.groupDeviceIds]);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  /** 待删除的分组 id——就地二次确认，不再套一层 AlertDialog（对话框里再开对话框是噪音）。 */
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  const [newName, setNewName] = useState("");

  const startEdit = (id: string, name: string) => {
    setEditingId(id);
    setDraft(name);
    setConfirmingId(null);
  };

  const commitEdit = () => {
    // 空名 = **取消**这次重命名（保持原名退出编辑），不是「改成空名」。
    //
    // store 侧也拦空，但判据必须在这里再写一遍：调用方不该拿被调用方的静默 no-op
    // 当作自己的交互语义——那样 store 哪天放宽成「允许空名」，这里就变成静默清名了，
    // 而且没有任何一处代码看得出来是谁负责这件事。
    if (editingId && draft.trim()) preferencesActions.renameGroup(editingId, draft);
    setEditingId(null);
  };

  const addGroup = () => {
    if (!newName.trim()) return;
    preferencesActions.createGroup(newName);
    setNewName("");
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            <Trans>管理分组</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>分组只保存在这台设备上，不会同步给对方，也不影响传输。</Trans>
          </DialogDescription>
        </DialogHeader>

        {groups.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            <Trans>还没有分组。设备多起来之后，分组能让「发给谁」这件事快一点。</Trans>
          </p>
        ) : (
          <ul className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
            {groups.map((group) => (
              <li key={group.id} className="rounded-lg border bg-background px-3 py-2">
                {editingId === group.id ? (
                  <div className="flex items-center gap-2">
                    <Input
                      value={draft}
                      autoFocus
                      aria-label={t`分组名`}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          commitEdit();
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          setEditingId(null);
                        }
                      }}
                      className="h-11 min-w-0 flex-1 sm:h-8"
                    />
                    <Button
                      size="sm"
                      onClick={commitEdit}
                      aria-label={t`保存分组名`}
                      className="shrink-0"
                    >
                      <Check className="size-3.5" aria-hidden />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setEditingId(null)}
                      aria-label={t`取消重命名`}
                      className="shrink-0"
                    >
                      <X className="size-3.5" aria-hidden />
                    </Button>
                  </div>
                ) : confirmingId === group.id ? (
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="min-w-0 text-xs text-muted-foreground">
                      {/* 说清楚代价：设备本身不受影响，丢的是「谁属于这一组」这份关系。 */}
                      <Trans>删掉「{group.name}」？设备不受影响，只是不再归在这一组里。</Trans>
                    </p>
                    <span className="flex shrink-0 gap-2">
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setConfirmingId(null)}
                                              >
                        <Trans>取消</Trans>
                      </Button>
                      <Button
                        size="sm"
                        onClick={() => {
                          preferencesActions.deleteGroup(group.id);
                          setConfirmingId(null);
                        }}
                        className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                      >
                        <Trans>删除</Trans>
                      </Button>
                    </span>
                  </div>
                ) : (
                  <div className="flex items-center justify-between gap-2">
                    <div className="min-w-0">
                      <p className="truncate text-sm text-foreground">{group.name}</p>
                      <p className="text-[11px] tabular-nums text-muted-foreground">
                        <Trans>{memberCount[group.id] ?? 0} 台设备</Trans>
                      </p>
                    </div>
                    <span className="flex shrink-0 gap-1">
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => startEdit(group.id, group.name)}
                        aria-label={t`重命名「${group.name}」`}
                                              >
                        <Pencil className="size-3.5" aria-hidden />
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          setConfirmingId(group.id);
                          setEditingId(null);
                        }}
                        aria-label={t`删除「${group.name}」`}
                        className="text-destructive-ink"
                      >
                        <Trash2 className="size-3.5" aria-hidden />
                      </Button>
                    </span>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}

        <div className="flex items-center gap-2 border-t pt-4">
          <Input
            value={newName}
            placeholder={t`新分组名`}
            aria-label={t`新分组名`}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addGroup();
              }
            }}
            className="h-11 min-w-0 flex-1 sm:h-9"
          />
          <Button
            size="sm"
            onClick={addGroup}
            disabled={!newName.trim()}
            className="shrink-0 gap-1.5"
          >
            <Plus className="size-4" aria-hidden />
            <Trans>新建</Trans>
          </Button>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            <Trans>完成</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
