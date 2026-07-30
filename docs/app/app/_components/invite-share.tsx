"use client";

// 生成邀请后的分享块：二维码 + 复制 + 手选兜底。Web 端最主要的配对姿势就是「浏览器出码、
// 手机扫」——两百多字符的 canonical 链接靠手抄传不过去，没有码这条路径等于不存在。
//
// **名字刻意不叫 `InviteQr`**：桌面端 `src/components/pairing/invite-qr.tsx` 与移动端的
// 同名组件严格只是码面（props 就 invite/size/overlay，复制按钮在页面的 CommandDock 里），
// 而这里装的是整块分享 UI。同名不同职责会让人按那边的心智来改这边。
//
// SVG 由 wasm 侧 `node.invite_qr_svg()` 生成，编码规范在 `crates/invite/src/qr.rs` 三端
// 单点固化（原样编码 + 最优分段 + ECL::M + quiet zone）。**不要改用 JS 二维码库**：
// 三端各画一遍，规范就会漂，而漂了的症状是「这端生成的码那端扫不出来」，极难归因。
//
// 码面固定深模块 + 白底、**不随暗色主题反色**（摄像头对反色 QR 识别差）。因此白卡内
// 的一切文字都得用固定深色，**不能**用 `text-fd-muted-foreground` 这类主题 token——
// 暗色主题下它会变浅灰，压在白底上不可读。

import { Check, Copy } from "lucide-react";
import { memo, useMemo, useRef, useState } from "react";
import { INVITE_TTL_HOURS } from "../_lib/invite";
import { getNode } from "../_lib/node-runtime";

/** 码面边长（px）。手机扫描距离 20–30cm 下够用，也不至于把面板撑开。 */
const QR_SIZE = 196;
/** 白卡内边距（px），须与下面的 `p-3` 一致——尺寸由它派生，改一处不会静默错位。 */
const CARD_PADDING = 12;

/**
 * `invite` 为 null 表示「正在生成」：白卡留在原位显示占位，**不卸载整块**。
 * 重新生成时若让整块消失再长回来，码面区域会塌一下——桌面端用覆盖层压住失效的码来
 * 避免这个跳版，这里用同尺寸占位达到同样效果。
 */
export const InviteShare = memo(function InviteShare({ invite }: { invite: string | null }) {
  // 同步计算：`invite_qr_svg` 是纯计算（不碰 IndexedDB、不碰网络），一帧就出结果，
  // 不需要桌面端那套 loading 态——那边的 loading 是跨 Tauri IPC 才有的。
  //
  // **连 `{__html}` 对象一起缓存**：React 对 `dangerouslySetInnerHTML` 只做引用比较，
  // 在 JSX 里现造对象字面量会让每次重渲染都重设一遍 innerHTML —— 这个码面的 SVG 有
  // 三万多字节、path 里一千六百多条子命令，重解析不便宜。而配对确认那几轮 store 更新
  // （对方扫完码拨过来）正好都落在用户盯着码面的时候。
  const html = useMemo(() => {
    if (invite === null) return null;
    try {
      // 空串不能进 `{__html}`——那个对象恒为真值，会渲染出一个既没码也没文字的空白卡。
      const svg = getNode()?.invite_qr_svg(invite);
      return svg ? { __html: svg } : null;
    } catch (e) {
      console.error("[web] 二维码生成失败", e);
      return null;
    }
  }, [invite]);

  return (
    <div className="mt-3 flex flex-col items-center gap-3 sm:flex-row sm:items-start">
      <div
        className="flex shrink-0 items-center justify-center rounded-xl bg-white p-3 ring-1 ring-slate-900/[0.06] dark:ring-white/15"
        style={{ width: QR_SIZE + CARD_PADDING * 2, height: QR_SIZE + CARD_PADDING * 2 }}
      >
        {html ? (
          <div
            role="img"
            aria-label="配对邀请二维码"
            className="size-full [&>svg]:size-full"
            // SVG 由 wasm 侧受信任生成（纯几何 path，无脚本），内联安全。
            dangerouslySetInnerHTML={html}
          />
        ) : (
          // 白卡内固定深色，理由见文件头
          <p className="px-2 text-center text-xs text-slate-500">
            {invite === null ? "正在生成…" : "二维码生成失败，请用下方链接配对"}
          </p>
        )}
      </div>

      <div className="flex w-full min-w-0 flex-col gap-2">
        {/* 扫码只有手机端做得到（桌面端无相机，见 `_app/pairing/input.lazy.tsx`），
            所以两条路径要分端说清楚，不能笼统写「扫码或粘贴」。 */}
        <p className="text-xs text-fd-muted-foreground">
          手机上用 SwarmDrop 的「扫码」对准它；桌面端没有相机，复制链接发过去用「粘贴邀请」。
          {INVITE_TTL_HOURS} 小时内有效。
        </p>
        {/* `key` 让按钮随邀请换代重挂：否则「已复制」会在点了「重新生成」之后继续挂着，
            而剪贴板里躺的是刚被撤销的上一条；「复制失败」同理会残留到一条全新的邀请上。 */}
        <CopyInviteButton key={invite ?? "pending"} invite={invite} />
        {/* 手选兜底：复制按钮在极少数环境下不可用（权限被拒），链接本身仍要够得着。
            一行截断而不是多行 textarea——它是第三顺位的路径，不该占主要视觉重量。 */}
        <input
          readOnly
          value={invite ?? ""}
          aria-label="邀请链接"
          onFocus={(e) => e.currentTarget.select()}
          className="w-full truncate rounded-lg border border-fd-border bg-fd-background px-2 py-1.5 font-mono text-xs text-fd-muted-foreground"
        />
      </div>
    </div>
  );
});

function CopyInviteButton({ invite }: { invite: string | null }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  // **同步的失败也要接住**：非安全上下文下 `navigator.clipboard` 直接是 undefined，
  // 取 `.writeText` 就是一次同步 TypeError——挂在 promise 上的 onRejected 根本轮不到，
  // 按钮会一动不动地骗人。而「非安全上下文」恰是复制不可用最常见的成因。
  const copy = async () => {
    if (invite === null) return;
    try {
      await navigator.clipboard.writeText(invite);
      setState("copied");
      // 只有「已复制」是一次性确认，该自动收回。放在 handler 里而不是 effect 里：
      // 依赖 state 的 effect 在连点时写入同值会被 React bail out，那 2 秒不重新计时。
      // 连点要顶掉上一个 timer，否则前一次的到点会把这一次的确认提前掐掉。
      clearTimeout(resetTimer.current);
      resetTimer.current = setTimeout(() => setState("idle"), 2000);
    } catch {
      // 不弹 toast（应用区没有 toast 系统），就地把失败说出来——下面那行输入框可以手选。
      // 失败态**不自动清**：2 秒后自己消失的错误提示等于没提示，留到下次点击时覆盖。
      clearTimeout(resetTimer.current);
      setState("failed");
    }
  };

  return (
    <button
      type="button"
      onClick={() => void copy()}
      disabled={invite === null}
      className="inline-flex w-fit items-center gap-1.5 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
    >
      {state === "copied" ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
      {state === "copied" ? "已复制" : state === "failed" ? "复制失败，请手动选中" : "复制链接"}
    </button>
  );
}
