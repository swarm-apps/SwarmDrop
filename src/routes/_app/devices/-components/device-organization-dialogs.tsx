import { useEffect, useState } from "react";
import { ChevronDown, ChevronUp, Plus, Trash2 } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import type { Device } from "@/lib/bindings";
import type { DeviceOrganization } from "@/lib/device-organization";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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

interface OrganizationActions {
  setAlias: (peerId: string, name: string) => void;
  setGroups: (peerId: string, groupIds: string[]) => void;
  createGroup: (name: string) => string | null;
  renameGroup: (groupId: string, name: string) => void;
  deleteGroup: (groupId: string) => void;
  reorderGroups: (groupIds: string[]) => void;
}

export function DeviceOrganizationDialog({
  open,
  onOpenChange,
  device,
  organization,
  actions,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  device: Device | null;
  organization: DeviceOrganization;
  actions: OrganizationActions;
}) {
  const [alias, setAlias] = useState("");
  const [groupIds, setGroupIds] = useState<string[]>([]);
  const [newGroup, setNewGroup] = useState("");

  useEffect(() => {
    if (!open || !device) return;
    setAlias(organization.aliases[device.peerId] ?? "");
    setGroupIds(
      Object.entries(organization.groupDeviceIds)
        .filter(([, peerIds]) => peerIds.includes(device.peerId))
        .map(([groupId]) => groupId),
    );
  }, [device, open, organization]);

  if (!device) return null;

  const toggleGroup = (groupId: string, checked: boolean) => {
    setGroupIds((current) => checked
      ? [...current, groupId]
      : current.filter((id) => id !== groupId));
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            actions.setAlias(device.peerId, alias);
            actions.setGroups(device.peerId, groupIds);
            onOpenChange(false);
          }}
        >
          <DialogHeader>
            <DialogTitle><Trans>设备别名与分组</Trans></DialogTitle>
            <DialogDescription>
              <Trans>这些信息仅保存在本机，不会同步给对端。</Trans>
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <label className="block text-sm font-medium text-foreground" htmlFor="device-alias">
              <Trans>设备别名</Trans>
              <Input id="device-alias" value={alias} onChange={(event) => setAlias(event.target.value)} className="mt-2" />
            </label>
            <fieldset className="space-y-2">
              <legend className="text-sm font-medium text-foreground"><Trans>所属分组</Trans></legend>
              {organization.groups.length === 0 ? (
                <p className="text-xs text-muted-foreground"><Trans>还没有分组，可在下方创建。</Trans></p>
              ) : organization.groups.map((group) => (
                <label key={group.id} className="flex cursor-pointer items-center gap-2 text-sm text-foreground">
                  <input type="checkbox" checked={groupIds.includes(group.id)} onChange={(event) => toggleGroup(group.id, event.target.checked)} />
                  {group.name}
                </label>
              ))}
            </fieldset>
            <div className="flex gap-2">
              <Input value={newGroup} onChange={(event) => setNewGroup(event.target.value)} placeholder="新分组名称" />
              <Button
                type="button"
                variant="outline"
                size="icon"
                aria-label="创建分组"
                onClick={() => {
                  const id = actions.createGroup(newGroup);
                  if (id) {
                    setGroupIds((current) => [...current, id]);
                    setNewGroup("");
                  }
                }}
              >
                <Plus className="size-4" />
              </Button>
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}><Trans>取消</Trans></Button>
            <Button type="submit"><Trans>保存</Trans></Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function DeviceGroupsDialog({
  open,
  onOpenChange,
  organization,
  actions,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  organization: DeviceOrganization;
  actions: Pick<OrganizationActions, "createGroup" | "renameGroup" | "deleteGroup" | "reorderGroups">;
}) {
  const [newGroup, setNewGroup] = useState("");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const groups = [...organization.groups].sort((a, b) => a.sortOrder - b.sortOrder);

  const move = (groupId: string, offset: number) => {
    const from = groups.findIndex((group) => group.id === groupId);
    const to = from + offset;
    if (from < 0 || to < 0 || to >= groups.length) return;
    const ids = groups.map((group) => group.id);
    [ids[from], ids[to]] = [ids[to], ids[from]];
    actions.reorderGroups(ids);
  };

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle><Trans>管理设备分组</Trans></DialogTitle>
            <DialogDescription><Trans>删除分组不会取消其中设备的配对。</Trans></DialogDescription>
          </DialogHeader>
          <div className="space-y-2 py-3">
            {groups.map((group, index) => (
              <div key={group.id} className="flex items-center gap-2">
                <Input defaultValue={group.name} onBlur={(event) => actions.renameGroup(group.id, event.target.value)} />
                <Button type="button" size="icon" variant="ghost" disabled={index === 0} onClick={() => move(group.id, -1)}><ChevronUp className="size-4" /></Button>
                <Button type="button" size="icon" variant="ghost" disabled={index === groups.length - 1} onClick={() => move(group.id, 1)}><ChevronDown className="size-4" /></Button>
                <Button type="button" size="icon" variant="ghost" onClick={() => setPendingDelete(group.id)}><Trash2 className="size-4" /></Button>
              </div>
            ))}
            <div className="flex gap-2 pt-2">
              <Input value={newGroup} onChange={(event) => setNewGroup(event.target.value)} placeholder="新分组名称" />
              <Button type="button" variant="outline" onClick={() => { if (actions.createGroup(newGroup)) setNewGroup(""); }}><Trans>创建</Trans></Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
      <AlertDialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle><Trans>删除分组</Trans></AlertDialogTitle>
            <AlertDialogDescription><Trans>分组内设备将保留为已配对状态，并显示在未分组列表中。</Trans></AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel><Trans>取消</Trans></AlertDialogCancel>
            <AlertDialogAction onClick={() => { if (pendingDelete) actions.deleteGroup(pendingDelete); setPendingDelete(null); }}><Trans>删除</Trans></AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
