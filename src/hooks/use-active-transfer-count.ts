import { useMemo } from "react";
import { isProjectionActive } from "@/lib/transfer-projection";
import { useTransferStore } from "@/stores/transfer-store";

/**
 * 当前在途（未终态、非暂停）的传输会话数。
 *
 * 停止或重启节点会断开全部连接，这些会话会当场中断——所以凡是能触发停机的入口
 * 都要先问一次这个数，把后果说出来。
 *
 * 只订阅 `projections` 引用、派生放 `useMemo`：高频的 progress 事件不改这个引用，
 * 计数只在会话状态真变时重算（`check:zustand-access` 的规则 B 也要求 selector 不派生）。
 */
export function useActiveTransferCount(): number {
  const projections = useTransferStore((s) => s.projections);
  return useMemo(
    () => Object.values(projections).filter(isProjectionActive).length,
    [projections],
  );
}
