import { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  DragOverlay,
  type DragStartEvent,
  KeyboardSensor,
  type Modifier,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Check, GripVertical, MonitorSmartphone, Plus, Tags, Trash2 } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { plural, t } from "@lingui/core/macro";
import type { Device } from "@/lib/bindings";
import {
  type DeviceGroup,
  type DeviceOrganization,
  sortGroups,
} from "@/lib/device-organization";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useMediaQuery } from "@/hooks/use-media-query";
import { CenteredEmptyState } from "@/components/layout/section-primitives";
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

/** 两个分组弹窗共用的"内凹分组面板"外观（padding 各自追加）。 */
const RECESSED_PANEL =
  "rounded-[16px] border border-border/60 bg-muted/20 dark:bg-white/[0.02]";

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
  const groupsLabelId = useId();

  // 仅在弹窗打开 / 切换设备时，按当时的 organization 快照重置本地编辑态。
  // organization 不能进依赖：弹窗内新建分组会 mutate store → organization 换引用，
  // 若重跑此 effect 会把用户尚未保存的别名、pill 勾选连同刚建的新分组一起冲掉。
  const organizationRef = useRef(organization);
  organizationRef.current = organization;
  useEffect(() => {
    if (!open || !device) return;
    const org = organizationRef.current;
    setAlias(org.aliases[device.peerId] ?? "");
    setGroupIds(
      Object.entries(org.groupDeviceIds)
        .filter(([, peerIds]) => peerIds.includes(device.peerId))
        .map(([groupId]) => groupId),
    );
    setNewGroup("");
  }, [device, open]);

  const sortedGroups = useMemo(
    () => sortGroups(organization.groups),
    [organization.groups],
  );

  if (!device) return null;

  const toggleGroup = (groupId: string) => {
    setGroupIds((current) =>
      current.includes(groupId)
        ? current.filter((id) => id !== groupId)
        : [...current, groupId],
    );
  };

  const handleCreate = () => {
    const id = actions.createGroup(newGroup);
    if (id) {
      setGroupIds((current) => [...current, id]);
      setNewGroup("");
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form
          onSubmit={(event) => {
            event.preventDefault();
            actions.setAlias(device.peerId, alias);
            actions.setGroups(device.peerId, groupIds);
            onOpenChange(false);
          }}
          className="space-y-5"
        >
          <DialogHeader>
            <DialogTitle><Trans>设备别名与分组</Trans></DialogTitle>
            <DialogDescription>
              <Trans>这些信息仅保存在本机，不会同步给对端。</Trans>
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-2">
            <label className="text-sm font-medium text-foreground" htmlFor="device-alias">
              <Trans>设备别名</Trans>
            </label>
            <Input
              id="device-alias"
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              placeholder={t`留空则使用设备名`}
            />
          </div>

          <div className="space-y-2">
            <span id={groupsLabelId} className="text-sm font-medium text-foreground"><Trans>所属分组</Trans></span>
            <div className={cn(RECESSED_PANEL, "p-3")}>
              {sortedGroups.length === 0 ? (
                <p className="py-1 text-center text-xs text-muted-foreground">
                  <Trans>还没有分组，可在下方创建。</Trans>
                </p>
              ) : (
                <div
                  className="flex flex-wrap gap-2"
                  role="group"
                  aria-labelledby={groupsLabelId}
                >
                  {sortedGroups.map((group) => {
                    const active = groupIds.includes(group.id);
                    return (
                      <button
                        type="button"
                        key={group.id}
                        onClick={() => toggleGroup(group.id)}
                        aria-pressed={active}
                        className={cn(
                          "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm font-medium transition-colors",
                          active
                            ? "border-primary/30 bg-primary/10 text-brand"
                            : "border-border/60 bg-background text-muted-foreground hover:bg-accent hover:text-foreground",
                        )}
                      >
                        {active && <Check className="size-3.5" />}
                        {group.name}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
            <div className="flex gap-2 pt-1">
              <Input
                value={newGroup}
                onChange={(event) => setNewGroup(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    handleCreate();
                  }
                }}
                placeholder={t`新分组名称`}
              />
              <Button
                type="button"
                variant="outline"
                disabled={!newGroup.trim()}
                onClick={handleCreate}
              >
                <Plus className="size-4" />
                <Trans>新建</Trans>
              </Button>
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              <Trans>取消</Trans>
            </Button>
            <Button type="submit"><Trans>保存</Trans></Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * 仅纵向拖拽：分组是垂直列表，锁死 x 轴避免拖动时左右漂移（等价 @dnd-kit/modifiers
 * 的 restrictToVerticalAxis，内联实现以免为一个函数引入整包依赖）。
 */
const restrictToVerticalAxis: Modifier = ({ transform }) => ({
  ...transform,
  x: 0,
});

export function DeviceGroupsDialog({
  open,
  onOpenChange,
  organization,
  actions,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  organization: DeviceOrganization;
  actions: Pick<
    OrganizationActions,
    "createGroup" | "renameGroup" | "deleteGroup" | "reorderGroups"
  >;
}) {
  const [newGroup, setNewGroup] = useState("");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const reduceMotion = useMediaQuery("(prefers-reduced-motion: reduce)");

  const groups = useMemo(
    () => sortGroups(organization.groups),
    [organization.groups],
  );
  const ids = useMemo(() => groups.map((group) => group.id), [groups]);
  const activeGroup = groups.find((group) => group.id === activeId) ?? null;

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragStart = (event: DragStartEvent) => {
    setActiveId(event.active.id as string);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveId(null);
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const from = ids.indexOf(active.id as string);
    const to = ids.indexOf(over.id as string);
    if (from < 0 || to < 0) return;
    actions.reorderGroups(arrayMove(ids, from, to));
  };

  const handleCreate = () => {
    if (actions.createGroup(newGroup)) setNewGroup("");
  };

  const deleteTarget = groups.find((group) => group.id === pendingDelete) ?? null;
  // 保留最后一次删除目标名：确认框关闭时 pendingDelete 立即为 null，用 ref 兜住名字，
  // 避免退出动画期间标题从「删除分组「X」」闪回通用「删除分组」。
  const lastDeleteName = useRef("");
  if (deleteTarget) lastDeleteName.current = deleteTarget.name;

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent
          className="flex max-h-[85vh] flex-col gap-0 sm:max-w-md"
          onEscapeKeyDown={(event) => {
            // 键盘拖拽进行中按 Esc 只取消这次排序（dnd-kit），别冒泡关掉整个弹窗
            if (activeId) event.preventDefault();
          }}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle><Trans>管理设备分组</Trans></DialogTitle>
            <DialogDescription>
              <Trans>拖动排序、重命名或删除分组。删除分组不会取消其中设备的配对。</Trans>
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 overflow-y-auto py-4">
            {groups.length === 0 ? (
              <CenteredEmptyState
                icon={Tags}
                title={<Trans>还没有分组</Trans>}
                description={<Trans>在下方创建第一个分组来整理已配对设备。</Trans>}
                className="min-h-[180px]"
              />
            ) : (
              <DndContext
                sensors={sensors}
                collisionDetection={closestCenter}
                modifiers={[restrictToVerticalAxis]}
                onDragStart={handleDragStart}
                onDragEnd={handleDragEnd}
                onDragCancel={() => setActiveId(null)}
              >
                <SortableContext items={ids} strategy={verticalListSortingStrategy}>
                  <ul className={cn(RECESSED_PANEL, "flex flex-col gap-0.5 p-1.5")}>
                    {groups.map((group) => (
                      <SortableGroupRow
                        key={group.id}
                        group={group}
                        deviceCount={organization.groupDeviceIds[group.id]?.length ?? 0}
                        reduceMotion={reduceMotion}
                        onRename={actions.renameGroup}
                        onRequestDelete={setPendingDelete}
                      />
                    ))}
                  </ul>
                </SortableContext>
                <DragOverlay dropAnimation={reduceMotion ? null : undefined}>
                  {activeGroup ? (
                    <GroupRowPreview
                      name={activeGroup.name}
                      deviceCount={
                        organization.groupDeviceIds[activeGroup.id]?.length ?? 0
                      }
                    />
                  ) : null}
                </DragOverlay>
              </DndContext>
            )}
          </div>

          <form
            className="flex shrink-0 items-center gap-2 border-t border-border/60 pt-4"
            onSubmit={(event) => {
              event.preventDefault();
              handleCreate();
            }}
          >
            <Input
              value={newGroup}
              onChange={(event) => setNewGroup(event.target.value)}
              placeholder={t`新分组名称`}
            />
            <Button type="submit" variant="outline" disabled={!newGroup.trim()}>
              <Plus className="size-4" />
              <Trans>新建</Trans>
            </Button>
          </form>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {lastDeleteName.current
                ? <Trans>删除分组「{lastDeleteName.current}」</Trans>
                : <Trans>删除分组</Trans>}
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>分组内设备将保留为已配对状态，并显示在未分组列表中。</Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel><Trans>取消</Trans></AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingDelete) actions.deleteGroup(pendingDelete);
                setPendingDelete(null);
              }}
            >
              <Trans>删除</Trans>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/** 成员数量徽章：图标 + 等宽数字，读屏读出完整「N 台设备」。可拖拽行与拖拽预览共用。 */
function GroupMemberCount({ deviceCount }: { deviceCount: number }) {
  const label = plural(deviceCount, { one: "# 台设备", other: "# 台设备" });
  return (
    <span
      className="inline-flex shrink-0 items-center gap-1 px-1 text-xs text-muted-foreground"
      title={label}
    >
      <MonitorSmartphone className="size-3.5" aria-hidden />
      <span className="font-mono tabular-nums" aria-hidden>{deviceCount}</span>
      <span className="sr-only">{label}</span>
    </span>
  );
}

function SortableGroupRow({
  group,
  deviceCount,
  reduceMotion,
  onRename,
  onRequestDelete,
}: {
  group: DeviceGroup;
  deviceCount: number;
  reduceMotion: boolean;
  onRename: (groupId: string, name: string) => void;
  onRequestDelete: (groupId: string) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: group.id });

  return (
    <li
      ref={setNodeRef}
      style={{
        transform: CSS.Transform.toString(transform),
        transition: reduceMotion ? undefined : transition,
      }}
      className={cn(
        "flex items-center gap-1 rounded-[10px] px-1.5 py-1.5 transition-colors hover:bg-accent/60",
        isDragging && "opacity-40",
      )}
    >
      <button
        type="button"
        className="flex size-7 shrink-0 cursor-grab touch-none items-center justify-center rounded-md text-muted-foreground/50 transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
        aria-label={t`拖动排序`}
        {...attributes}
        {...listeners}
      >
        <GripVertical className="size-4" />
      </button>
      <Input
        defaultValue={group.name}
        onBlur={(event) => onRename(group.id, event.target.value)}
        aria-label={t`分组名称`}
        className="h-8 flex-1 border-transparent bg-transparent px-2 text-sm shadow-none focus-visible:border-input focus-visible:bg-background"
      />
      <GroupMemberCount deviceCount={deviceCount} />
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        className="shrink-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
        aria-label={t`删除分组「${group.name}」`}
        onClick={() => onRequestDelete(group.id)}
      >
        <Trash2 className="size-4" />
      </Button>
    </li>
  );
}

/**
 * 拖拽悬浮预览：dnd-kit DragOverlay 以 position:fixed 渲染，脱离弹窗滚动容器不被裁切，
 * 呈现「被拎起」的抬升态（primary 描边 + 玻璃投影）。纯展示，不含可交互控件。
 */
function GroupRowPreview({
  name,
  deviceCount,
}: {
  name: string;
  deviceCount: number;
}) {
  return (
    <div className="flex cursor-grabbing items-center gap-1 rounded-[10px] border border-primary/25 bg-popover px-1.5 py-1.5 shadow-[0_16px_40px_rgb(15_23_42_/_0.16)] dark:shadow-[0_20px_48px_rgb(0_0_0_/_0.5)]">
      <span className="flex size-7 shrink-0 items-center justify-center rounded-md text-brand">
        <GripVertical className="size-4" />
      </span>
      <span className="flex-1 truncate px-2 text-sm font-medium text-foreground">
        {name}
      </span>
      <GroupMemberCount deviceCount={deviceCount} />
      <span className="flex size-7 shrink-0 items-center justify-center text-muted-foreground">
        <Trash2 className="size-4" />
      </span>
    </div>
  );
}
