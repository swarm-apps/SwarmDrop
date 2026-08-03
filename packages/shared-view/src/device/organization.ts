/**
 * 设备组织 —— 本机对已配对设备的**别名与分组**的显示投影。
 *
 * 别名 / 分组只保存在本机偏好里，不写入 keychain 的 `PairedDeviceInfo`，也不同步给对端。
 * 本模块是纯投影：由「设备 + 组织」算出「显示成什么」，不碰持久化。
 *
 * ## 收口时收敛掉的三处三端分叉
 *
 * 桌面（`src/lib/device-organization.ts`，77 行）与移动（`mobile/src/lib/device-organization.ts`，
 * 150 行）此前各有一份，已经开始漂移。逐条的取舍：
 *
 * | | 桌面原样 | 移动原样 | 这里取 | 理由 |
 * |---|---|---|---|---|
 * | `deviceIdentityHint` 的 hostname 缺省 | `未知设备 · 短ID` | `短ID · 短ID` | 只给 `短ID` | 见下 |
 * | 分组排序的并列兜底 | `sortOrder` → `name` | 只有 `sortOrder` | 带 `name` 兜底 | 见下 |
 * | `normalizeDeviceOrganization` | 在 `stores/preferences-store.ts` | 在本模块 | 收到这里 | 两份实现同构 |
 *
 * **`deviceIdentityHint` 的 hostname 缺省**：桌面那句 `"未知设备"` 是**硬编码中文，写在一个
 * 声明为 i18n-free 的模块里**——它不可能跟着 locale 走，搬进共享包更不可能（包里没有 i18n
 * 运行时）。移动那版则退化成 `短ID · 短ID`，把同一个串说了两遍。两者在 hostname 为空时都是
 * 坏的。这里的取法是**不摆占位文案**：提示的用途本就是「拿什么区分同名设备」，短 PeerId
 * 自己就够了。实际影响面为零——hostname 来自内核 OsInfo，是系统主机名，跑到这条分支说明
 * 上游已经出问题了。
 *
 * **分组排序的并列兜底**：两个分组 `sortOrder` 相同时，移动那版落到 `Array.sort` 的稳定性
 * （即插入顺序），桌面那版按名称。取**带名称兜底**的那版——因为两端的 `deviceGroupNames`
 * 本来就都带它，不带的只有移动端那个独立的分组排序函数。改成不带会同时动到两端已经一致的
 * 行为，改成带只动移动端那一个离群点。
 *
 * 注意这条兜底给的是「同一运行时内可预测」，**不是字节级的跨端一致**：`localeCompare()`
 * 不传 locale 时用运行时默认排序规则，三端（V8 / 浏览器 / Hermes）未必给出同一个次序
 * （实测 `"乙".localeCompare("甲")` 走的是码点而非拼音）。真要跨端严格一致得改成码点比较，
 * 但那会同时改掉两端现有的 `deviceGroupNames` 输出，超出本次收口的范围——
 * 而 `sortOrder` 并列本身只在持久化数据被手改过时才出现。
 */

/** 任何含 `{ peerId, hostname, name? }` 形状的设备快照——三端各自的类型都能结构化赋值。 */
export interface IdentifiedDevice {
  peerId: string;
  hostname: string;
  name?: string | null;
}

export interface DeviceGroup {
  id: string;
  name: string;
  /** 用户自定义顺序；并列时按 `name` 兜底，见模块头部的取舍表。 */
  sortOrder: number;
}

export interface DeviceOrganization {
  /** PeerId → 本机别名（已 trim，非空）。 */
  aliases: Record<string, string>;
  groups: DeviceGroup[];
  /** 分组 id → 该组内的 PeerId 列表。 */
  groupDeviceIds: Record<string, string[]>;
}

export const emptyDeviceOrganization: DeviceOrganization = {
  aliases: {},
  groups: [],
  groupDeviceIds: {},
};

/** PeerId 的短形式，`12D3…UvWxYz`。太短则原样返回（截了反而更难辨认）。 */
export function shortPeerId(peerId: string): string {
  return peerId.length > 10 ? `${peerId.slice(0, 4)}…${peerId.slice(-6)}` : peerId;
}

/**
 * 显示名的优先级：本机别名 → 对端 name → hostname → 短 PeerId。
 *
 * 与 [`deviceDisplayName`](./name.ts) 的分工：那个只认「对端 name / hostname」两级，用于
 * 拿不到本机组织的场合；这个多一级本机别名，是**设备列表里该显示的那个名字**。
 */
export function organizedDeviceName(
  device: IdentifiedDevice,
  organization: DeviceOrganization,
): string {
  return (
    organization.aliases[device.peerId]?.trim() ||
    device.name?.trim() ||
    device.hostname ||
    shortPeerId(device.peerId)
  );
}

/**
 * 次级身份提示 —— 同名消歧用的一行。
 *
 * hostname 为空时只给短 PeerId，**不摆占位文案**（理由见模块头部）。
 */
export function deviceIdentityHint(device: IdentifiedDevice): string {
  const short = shortPeerId(device.peerId);
  return device.hostname ? `${device.hostname} · ${short}` : short;
}

/**
 * 分组排序 —— 全 app 唯一的分组顺序入口，各处不要再手写 sort。
 *
 * `sortOrder` 并列时按 `name` 兜底（跨运行时的排序规则差异见模块头部的说明）。
 */
export function sortDeviceGroups(groups: DeviceGroup[]): DeviceGroup[] {
  return [...groups].sort(
    (a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name),
  );
}

/** 该设备所属的分组名，按 [`sortDeviceGroups`] 的顺序。 */
export function deviceGroupNames(
  peerId: string,
  organization: DeviceOrganization,
): string[] {
  const groupIds = new Set(
    Object.entries(organization.groupDeviceIds)
      .filter(([, deviceIds]) => deviceIds.includes(peerId))
      .map(([groupId]) => groupId),
  );
  return sortDeviceGroups(organization.groups.filter((group) => groupIds.has(group.id)))
    .map((group) => group.name);
}

/**
 * 该设备的显示名是否在给定列表中重复 —— 决定要不要展示次级身份提示。
 *
 * 注意入参 `devices` 应当是**当前正在渲染的那一批**，不是全量设备：分组视图里两台同名设备
 * 分处两组时并不构成歧义。
 */
export function hasDuplicateOrganizedName(
  device: IdentifiedDevice,
  devices: IdentifiedDevice[],
  organization: DeviceOrganization,
): boolean {
  const name = organizedDeviceName(device, organization);
  let seen = 0;
  for (const item of devices) {
    if (organizedDeviceName(item, organization) !== name) continue;
    if (++seen > 1) return true;
  }
  return false;
}

/**
 * 把任意持久化值归一成合法的 `DeviceOrganization`。
 *
 * 偏好来自磁盘（桌面 tauri-plugin-store / 移动 AsyncStorage / Web localStorage），可能是旧版本
 * 写的、可能缺字段、也可能被手改坏。这里一律降级而不抛：丢弃非法分组、剔除指向已删分组的
 * 成员关系、剔除空别名。拿不出任何有效内容就退回空组织。
 */
export function normalizeDeviceOrganization(value: unknown): DeviceOrganization {
  if (!value || typeof value !== "object") return { aliases: {}, groups: [], groupDeviceIds: {} };

  const source = value as Record<string, unknown>;
  const groups = Array.isArray(source.groups)
    ? source.groups.flatMap((group, index) => {
        if (!group || typeof group !== "object") return [];
        const candidate = group as Record<string, unknown>;
        if (typeof candidate.id !== "string" || typeof candidate.name !== "string") return [];
        return [
          {
            id: candidate.id,
            name: candidate.name,
            // 缺 sortOrder 的旧数据按它在数组里的位置补——保住用户当初排的那个顺序。
            sortOrder: typeof candidate.sortOrder === "number" ? candidate.sortOrder : index,
          },
        ];
      })
    : [];

  const groupIds = new Set(groups.map((group) => group.id));
  const aliases = Object.fromEntries(
    Object.entries(asRecord(source.aliases)).filter(
      (entry): entry is [string, string] =>
        typeof entry[1] === "string" && entry[1].trim().length > 0,
    ),
  );
  const groupDeviceIds = Object.fromEntries(
    Object.entries(asRecord(source.groupDeviceIds))
      .filter(([groupId]) => groupIds.has(groupId))
      .map(([groupId, peerIds]) => [
        groupId,
        Array.isArray(peerIds) ? peerIds.filter((id): id is string => typeof id === "string") : [],
      ]),
  );

  return { aliases, groups, groupDeviceIds };
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : {};
}
