"use client";

// 一条传输会话的标题行。三个调用点：传输列表行、传输详情侧、设备页的「活跃传输」区块。
//
// **独立成文件是为了第三个调用点。** 它原本住在 `transfer-detail.tsx` 里，而那个文件装着
// 整块传输详情面板（逐文件进度、链路证据、一堆动作）。设备页只想要这一个标题组件，却会因为
// 这条 import 把整块详情拖进自己的 bundle——设备页是应用首页，也是最不该为传输详情付费的那一页。

import { useLingui } from "@lingui/react/macro";
import type { TransferProjection } from "../_lib/view-types";

/**
 * 会话标题：首个文件名 + 「还有几个」的计数徽标。
 *
 * 计数**不并进被截断的那段文字**（「a.zip 等 3 个文件」在窄列里必然被切掉尾巴，而尾巴才是
 * 计数）——它自己占一个 `shrink-0` 的位，永远看得见。
 *
 * `files` 为空只可能出现在异常投影上，那时回落到调用方给的 `fallback`（对端名或一句占位），
 * 至少还认得出是跟谁的会话。
 */
export function SessionTitle({
  files,
  fallback,
}: {
  files: TransferProjection["files"];
  fallback: string;
}) {
  const { t } = useLingui();
  const first = files[0];
  const rest = files.length - 1;

  return (
    <p className="flex min-w-0 flex-1 items-center gap-1.5 text-xs font-medium text-foreground">
      <span className="truncate" title={first?.name ?? fallback}>
        {first?.name ?? fallback}
      </span>
      {rest > 0 && (
        <span
          className="shrink-0 rounded-full bg-muted px-1.5 text-[10px] font-normal text-muted-foreground"
          title={t`共 ${files.length} 个文件`}
        >
          +{rest}
        </span>
      )}
    </p>
  );
}
