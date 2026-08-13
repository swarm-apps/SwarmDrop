import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import type { ConnectionType } from "@/lib/bindings";

/**
 * 连接方式的一句话结论，**桌面端唯一一份**——设备卡的徽标与发送页的设备行都读它。
 *
 * `直连` 与 `打洞` 为什么必须是两个词、四个词为什么必须三端逐字一致：见
 * DESIGN.md 的 Slot 6 vocabulary 与 `crates/host` 的 `ConnectionType`。
 *
 * # 为什么单独一个文件，而不是搁在 `connection-badge.tsx` 里
 *
 * 那个文件顶层 import 了 Radix 的 Popover 与 Tooltip（徽标点开的链路详情要用）。
 * 标签表放在那里，`share-target.lazy.tsx` 只为查一张 4 项的表就把两个 Radix 包
 * 拖进自己的懒加载块——而它是「用 SwarmDrop 打开」的外部入口，恰是最该冷启动快的
 * 那条路由。本模块零组件依赖，两处各取所需。
 *
 * 这也是 Web 端 `docs/app/app/_components/transfer-labels.ts` 已有的分层：
 * 纯 `msg` 描述符独立成文件，组件那边只负责展开。
 *
 * ⚠️ **不进 `@swarmdrop/shared-view`**：那个包的归属判据第一条是「零平台依赖，
 * 不碰 i18n 运行时」，而这里存的正是 Lingui 描述符。三端各存一份是那条判据的
 * 既定结果，不是遗漏。
 */
export const CONNECTION_LABEL: Record<ConnectionType, MessageDescriptor> = {
  lan: msg`局域网`,
  direct: msg`直连`,
  dcutr: msg`打洞`,
  relay: msg`中继`,
};
