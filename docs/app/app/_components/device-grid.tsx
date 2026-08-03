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

import { Trans } from "@lingui/react/macro";
import { useMemo, useState } from "react";
import { deviceGroupNames, organizedDeviceName } from "@swarmdrop/shared-view";
import { usePreferences } from "../_lib/preferences-store";
import { useWebNode } from "../_lib/store";
import type { Device } from "../_lib/view-types";
import { DeviceCard } from "./device-card";
import { DeviceOrganizationDialog } from "./device-organization-dialog";
import { TrustPolicyDialog } from "./trust-policy-dialog";

export function DeviceGrid() {
  const devices = useWebNode((s) => s.pairedDevices);
  const organization = usePreferences((s) => s.deviceOrganization);
  // 两个对话框都是**列表级单例**：每张卡各挂一份会让 N 台设备生成 N 个 portal 容器，
  // 而同一时刻只可能开一个。
  const [organizing, setOrganizing] = useState<Device | null>(null);
  const [editingPolicy, setEditingPolicy] = useState<Device | null>(null);

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

  if (rows.length === 0) {
    return (
      <div className="rounded-xl border border-dashed bg-card/50 px-6 py-10 text-center">
        <p className="text-sm font-medium text-foreground">
          <Trans>还没有已配对的设备</Trans>
        </p>
        <p className="mx-auto mt-1 max-w-sm text-xs text-muted-foreground">
          {/* 空态的教学文案说「怎么让它变得非空」，而不是复述「这里是空的」。 */}
          <Trans>
            在下方「配对」区生成一条邀请发给对方，或把对方给你的邀请粘进去。配对是一次性动作，
            配完即长期信任。
          </Trans>
        </p>
      </div>
    );
  }

  return (
    <>
      {/*
        移动优先的响应式网格：<640 单列 · 640–919 两列 · ≥920 三列。
        920 不是随手挑的——它是三端统一的主从断点（`MASTER_DETAIL_QUERY`），
        设备网格在这里升到三列，与收件箱/传输在同一个宽度上升成双栏，整个应用区一起换形态。
      */}
      <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 min-[920px]:grid-cols-3">
        {rows.map(({ device, groupNames, showIdentityHint }) => (
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
      </ul>

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
