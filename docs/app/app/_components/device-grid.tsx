"use client";

// 已配对设备网格 —— 设备页的主体。
//
// 这里做三件卡片自己做不了的事：
//   1. 从组织派生每张卡要的分组名与「是否同名」，**一次算完**而不是每张卡各遍历一遍列表；
//   2. 排序（在线优先，其次显示名）；
//   3. 持有别名/分组对话框的开关——它是列表级的单例，不该每张卡各挂一个。
//
// presence 快照来自 `state-poll.ts` 的定时刷新，非事件驱动（`paired_devices()` 是同步查询
// 不是事件流）。

import { Trans, useLingui } from "@lingui/react/macro";
import { MonitorSmartphone, Plus, Tags } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  deviceGroupNames,
  organizedDeviceName,
  sortDeviceGroups,
} from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { usePreferences } from "../_lib/preferences-store";
import { useWebNode } from "../_lib/store";
import type { Device } from "../_lib/view-types";
import { DeviceCard } from "./device-card";
import { DeviceOrganizationDialog } from "./device-organization-dialog";
import { CenteredEmptyState, RailEmptyHint } from "./empty-state";
import { GroupManagerDialog } from "./group-manager-dialog";
import { NodeNotReadyState } from "./node-not-ready-state";
import { SectionHeader, SectionShell } from "./section";
import { TrustPolicyDialog } from "./trust-policy-dialog";

/**
 * 网格末尾的「添加设备」卡片——配对的入口。
 *
 * ## 为什么它长在网格里，而不是页头的一个按钮
 *
 * 配对要回答的问题是「我还想让哪台设备进这个列表」，而那个列表就在眼前。入口长在它末尾，
 * 位置本身就说明了会发生什么；页头按钮离列表最远，反而要靠文案解释。
 *
 * ## 为什么是虚线描边而不是一张实心卡
 *
 * 它是**占位**不是**内容**：实心卡片会读成「已经有这么一台设备」。虚线 + 无玻璃底
 * （`glass-panel` 留给真的承载内容的面）是通行的「这一格还空着」的写法，
 * 与 DESIGN.md「交互控件一律 flat，玻璃留给结构 chrome」也一致。
 *
 * 几何**逐项对齐 DeviceCard**（`rounded-xl` / `p-4` / `min-h-[136px]`）：它要占的是网格里
 * 与真卡片等价的一格，圆角或内边距差一点就会看出来是两种东西。
 * ⚠️ 别写成 `rounded-[var(--radius-card)]` / `p-[var(--space-card)]`——**那两个变量不存在**
 * （`global.css` 里只有 `--radius-panel{,-sm}` 与 `--space-{in-group,in-panel,panel,section}`），
 * 写了不会报错，只会静默解析失败、圆角和内边距一起归零。
 */
function AddDeviceCard({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="focus-ring group flex min-h-[136px] w-full flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border/70 p-4 text-muted-foreground transition-colors hover:border-[var(--brand)]/50 hover:bg-accent hover:text-foreground"
    >
      <Plus
        className="size-6 transition-colors group-hover:text-[var(--brand)]"
        aria-hidden="true"
      />
      <span className="text-sm font-medium">
        <Trans>添加设备</Trans>
      </span>
      <span className="text-xs">
        <Trans>互换一次邀请</Trans>
      </span>
    </button>
  );
}

/** 分组筛选的胶囊。`aria-pressed` 而非 `radio`：它过滤的是同一份列表，不是切换视图。 */
function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "focus-ring min-h-11 rounded-full border px-3 text-xs font-medium transition-colors sm:min-h-8",
        active
          ? "border-transparent bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-accent",
      )}
    >
      {children}
    </button>
  );
}

/** 分组筛选的取值：全部 / 未分组 / 某个分组 id。 */
type GroupFilter = "all" | "ungrouped" | (string & {});

export interface DeviceGridProps {
  /**
   * 「添加设备」被按下——由 `DevicesSection` 转成对配对面板的 `open()`。
   *
   * 网格自己不渲染配对表单：那是一整个面板（邀请输入 + 生成 + 已发出清单），塞进
   * `minmax(280px,1fr)` 的卡片轨道里只会把它压成一条缝。卡片是**入口**，落点在下面。
   */
  onAddDevice: () => void;
}

export function DeviceGrid({ onAddDevice }: DeviceGridProps) {
  const { t } = useLingui();
  const devices = useWebNode((s) => s.pairedDevices);
  const ready = useWebNode((s) => s.status === "running");
  const organization = usePreferences((s) => s.deviceOrganization);
  // 三个对话框都是**列表级单例**：每张卡各挂一份会让 N 台设备生成 N 个 portal 容器，
  // 而同一时刻只可能开一个。
  const [organizing, setOrganizing] = useState<Device | null>(null);
  const [editingPolicy, setEditingPolicy] = useState<Device | null>(null);
  const [managingGroups, setManagingGroups] = useState(false);
  const [filter, setFilter] = useState<GroupFilter>("all");

  const groups = useMemo(
    () => sortDeviceGroups(organization.groups),
    [organization.groups],
  );

  // 用户删掉当前正在筛选的那个组之后，筛选值会指向一个不存在的 id——列表随即空掉，
  // 而筛选条上那一档也已经消失，用户找不到「怎么退回全部」。退回 all。
  useEffect(() => {
    if (filter === "all" || filter === "ungrouped") return;
    if (!groups.some((g) => g.id === filter)) setFilter("all");
  }, [filter, groups]);

  // selector 只返回 store 内的稳定引用，派生一律放这里——`pnpm check:zustand-access` 规则 B。
  const rows = useMemo(() => {
    // 显示名算一次就够：排序、同名判定、卡片都要它，而它每次都要走「别名 → name → hostname
    // → 短 PeerId」四级回退。共享包的 `hasDuplicateOrganizedName` 是给「只判一台」的场合用的，
    // 整表遍历时逐台调它是 O(n²) 次名字计算——这里改成先算名字、再数一遍重名。
    const named = devices.map((device) => ({
      device,
      name: organizedDeviceName(device, organization),
    }));

    const nameCount = new Map<string, number>();
    for (const { name } of named) nameCount.set(name, (nameCount.get(name) ?? 0) + 1);

    // 在线优先：离线设备做不了任何事，让它们沉底。同为在线时按显示名，顺序才稳定。
    named.sort((a, b) => {
      const aOnline = a.device.status === "online";
      if (aOnline !== (b.device.status === "online")) return aOnline ? -1 : 1;
      return a.name.localeCompare(b.name);
    });

    return named.map(({ device, name }) => ({
      device,
      groupNames: deviceGroupNames(device.peerId, organization),
      // 同名判定只在**当前这批**里做：两台同名设备各在一组时并不构成歧义。
      showIdentityHint: (nameCount.get(name) ?? 0) > 1,
    }));
  }, [devices, organization]);

  // 筛选与排序分开两个 memo：换筛选不该连排序（含每台设备的四级名字回退）一起重跑。
  const visibleRows = useMemo(() => {
    if (filter === "all") return rows;
    if (filter === "ungrouped") return rows.filter((r) => r.groupNames.length === 0);
    const members = new Set(organization.groupDeviceIds[filter] ?? []);
    return rows.filter((r) => members.has(r.device.peerId));
  }, [rows, filter, organization.groupDeviceIds]);

  return (
    <>
      <SectionShell>
        <SectionHeader
          icon={MonitorSmartphone}
          title={<Trans>已配对设备</Trans>}
          count={rows.length > 0 ? rows.length : undefined}
          action={
            rows.length > 0 ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setManagingGroups(true)}
                className="gap-1.5 text-xs"
              >
                <Tags className="size-3.5" aria-hidden />
                <Trans>管理分组</Trans>
              </Button>
            ) : undefined
          }
        />

        {/* 分组筛选。只在真的有分组时出现——一台设备都没分组时，一条只有「全部」的
            筛选条是纯粹的版面开销。 */}
        {rows.length > 0 && groups.length > 0 && (
          <div
            role="group"
            aria-label={t`按分组筛选`}
            className="flex flex-wrap gap-1.5"
          >
            <FilterChip active={filter === "all"} onClick={() => setFilter("all")}>
              <Trans>全部</Trans>
            </FilterChip>
            {groups.map((group) => (
              <FilterChip
                key={group.id}
                active={filter === group.id}
                onClick={() => setFilter(group.id)}
              >
                {group.name}
              </FilterChip>
            ))}
            <FilterChip active={filter === "ungrouped"} onClick={() => setFilter("ungrouped")}>
              <Trans>未分组</Trans>
            </FilterChip>
          </div>
        )}

        {rows.length === 0 ? (
          // **「节点还没起来」不等于「你没有设备」**：`pairedDevices` 初值是空数组，而状态轮询
          // 要等 wasm 拉完 `_bg.wasm` 才第一次 tick。此前这里只看 `rows.length`，于是每次刷新，
          // 已配对的老用户都先看到一句确定性的「还没有已配对的设备」加一整段配对教学。
          !ready ? (
            // `fill={false}`：这是**面板里的一段**，不是一栏的全部内容。撑满会在设备网格
            // 该在的位置留出一块 320px 的空腔，一屏三块都这样就是三个等大的空盒子。
            <NodeNotReadyState
              fill={false}
              description={<Trans>节点起来后，已配对的设备会出现在这里。</Trans>}
            />
          ) : (
            <CenteredEmptyState
              fill={false}
              icon={MonitorSmartphone}
              title={<Trans>还没有已配对的设备</Trans>}
              // 教学文案说「怎么让它变得非空」。**不复述页头那句话**——页头已经说过
              //「配对是一次性动作，配完即长期信任」，在同一屏里再说一遍是纯噪音。
              //
              // 也不再说「用下面的『配对』区」：那句话预设配对面板常驻在下方，而它现在
              // 默认收起、由这里的按钮唤出。**空态的出口要是能按的东西，不是指路**。
              description={<Trans>生成一条邀请发给对方，或把对方给你的邀请粘进来。</Trans>}
              action={
                <Button onClick={onAddDevice} className="gap-1.5">
                  <Plus className="size-4" aria-hidden="true" />
                  <Trans>添加设备</Trans>
                </Button>
              }
            />
          )
        ) : (
          /*
            设备网格按**卡片自己的最小可用宽度**分列，不挂断点。

            ## 为什么不再写 `sm:grid-cols-2 min-[920px]:grid-cols-3`

            两个原因，各自都足够：

            1. **那句话里的第二半从来没生效过。** Tailwind v4 把任意值断点 `min-[…]` 排在
               具名断点**之前**，于是 1440px 下 `sm:`(640) 与 `min-[920px]:` 同时匹配、
               `sm:` 在后面赢，网格永远是两列。实测：`min-[920px]:grid-cols-3` 单独用得到
               三列，与 `sm:grid-cols-2` 并列就只剩两列。**并列写这两族断点是错的**，
               要么全用具名（`lg:`），要么像这里一样不用断点。
               （`src/` 里那几处是 `min-[920px]:` + `lg:`，顺序恰好就是想要的，不受影响。）
            2. **就算它生效了，结果也不对。** 视口 920 时内容列 ≈ 608px，三列 = 每张
               189px —— Device Card Contract 要求八项信息位，189px 装不下。

            `auto-fill` 而不是 `auto-fit`：后者会把空轨道折叠掉，于是**只有一台设备时那张卡
            会横跨整行**（1098px 宽的卡片，里面一条 1098px 的实心发送按钮）。auto-fill 保留
            轨道，一张卡就老实占一格。

            280px 是卡片的下限：图标 40 + 名称/状态 + 两枚徽标一行 + 发送按钮，再窄徽标就换行。
          */
          <>
            {visibleRows.length === 0 ? (
              // 「这个分组是空的」与「你一台设备都没有」是两件事，说法要分开——
              // 后者要教学，前者只需要一条退路。
              <RailEmptyHint>
                <Trans>这个分组下还没有设备。</Trans>
              </RailEmptyHint>
            ) : (
              <ul className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-[var(--space-in-group)]">
                {visibleRows.map(({ device, groupNames, showIdentityHint }) => (
                  <li key={device.peerId} className="flex">
                    <DeviceCard
                      device={device}
                      organization={organization}
                      groupNames={groupNames}
                      showIdentityHint={showIdentityHint}
                      onOrganize={setOrganizing}
                      onEditPolicy={setEditingPolicy}
                    />
                  </li>
                ))}
                {/* 只在「全部」下出现：分组筛选里它会读成「往这个分组加设备」，而配对与分组
                    毫无关系——加完仍然要手动归组。
                    筛选态下唯一的配对入口因此是下方那块面板（默认收起）。这是**刻意的**：
                    正在按分组看设备的人不是在找「怎么加一台新的」。 */}
                {filter === "all" && (
                  <li className="flex">
                    <AddDeviceCard onClick={onAddDevice} />
                  </li>
                )}
              </ul>
            )}
          </>
        )}
      </SectionShell>

      <GroupManagerDialog open={managingGroups} onOpenChange={setManagingGroups} />

      <DeviceOrganizationDialog
        device={organizing}
        onOpenChange={(open) => {
          if (!open) setOrganizing(null);
        }}
      />
      <TrustPolicyDialog
        device={editingPolicy}
        onOpenChange={(open) => {
          if (!open) setEditingPolicy(null);
        }}
      />
    </>
  );
}
