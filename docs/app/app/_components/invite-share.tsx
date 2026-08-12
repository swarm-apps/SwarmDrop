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

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { Check, Copy } from "lucide-react";
import { Ban, CheckCircle2, TimerOff, WifiOff, type LucideIcon } from "lucide-react";
import { memo, useMemo } from "react";
import { remainingLabel, remainingSeconds } from "../_lib/invite";
import { useCopyToClipboard } from "../_lib/use-copy";
import { getNode } from "../_lib/node-runtime";

/**
 * 码面边长（px）。手机扫描距离 20–30cm 下够用，也不至于把面板撑开。
 *
 * ⚠️ **它同时是后端的地址预算**：这个数直接传给 `invite_qr_svg`，wasm 侧按
 * `QR_SIZE / 2px每模块` 决定这张码放得下几条地址提示，放不下时按价值反序回收
 * （circuit 永不丢）。所以**改小它 = 邀请里的可拨路径变少**，不是渲染变糊。
 *
 * 以前这里还得手工与 Rust 侧一个 `INVITE_QR_MAX_MODULES = 98` 保持一致，且没有任何门禁
 * 会拦——那条对应关系已经取消，各端传自己的数即可。链接不受影响：那条路径没有密度上限。
 */
const QR_SIZE = 196;
/** 白卡内边距（px），须与下面的 `p-3` 一致——尺寸由它派生，改一处不会静默错位。 */
const CARD_PADDING = 12;

/** 码失效的四种成因。文案与图标在组件内映射，调用方只报事实不拼话。 */
type OverlayKind = "consumed" | "expired" | "revoked" | "unreachable";

interface InviteShareProps {
  /** `null` 表示「正在生成」：白卡留在原位显示占位，**不卸载整块**。 */
  invite: string | null;
  /**
   * 该邀请的失效时刻（Unix 秒，字符串承载——wasm 侧避开 u64 → BigInt）。
   * 调用方须 latch 住它，不能从只含未过期条目的列表里现查（理由见 pairing-panel）。
   */
  expiresAt: string | null;
  /** 邀请已被对方消费（一次性凭证，用掉即止）。 */
  consumed: boolean;
  /** 用户是否已在「已发出的邀请」里撤销了这一条。 */
  revoked: boolean;
  /** 本机当前是否有可达路径（relay reservation）。 */
  reachable: boolean;
  /**
   * 当前 Unix 秒，由调用方的 `useNowSeconds()` 提供。
   *
   * 时钟从外面传进来而不是本组件自己起一个：本组件是 `memo` 的，自建时钟只会推动
   * 自己，而同一个面板底部「已发出的邀请」列表里的剩余期由父组件渲染——两个时钟意味着
   * 两个相位，也意味着父组件那份可能整个不走。
   */
  now: number;
}

/**
 * 码失效后**必须在码面上说出来**（#101）。二维码与纯文本链接的差别就在这里：一段死文本
 * 粘过去会得到一句报错，而一个死码对方要先掏出手机、对准、等识别，全程没有任何信号说明
 * 它已经没用了——代价高得多。
 *
 * 所以失效时不撤掉码，而是把它压到几乎扫不出来再叠一层说明：位置不变、无布局跳动，
 * 且「这里本来有个码、现在它坏了」这件事本身是可见的。同桌面端 `InviteQrOverlay`。
 */
export const InviteShare = memo(function InviteShare({
  invite,
  expiresAt,
  consumed,
  revoked,
  reachable,
  now,
}: InviteShareProps) {
  const { t, i18n } = useLingui();
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
      const svg = getNode()?.invite_qr_svg(invite, QR_SIZE);
      return svg ? { __html: svg } : null;
    } catch (e) {
      console.error("[web] 二维码生成失败", e);
      return null;
    }
  }, [invite]);

  // **判过期用的是邀请自带的 `expiresAt` 绝对时刻**，不是「生成时刻 + TTL」——后者依赖
  // 前端记的那个时间点，刷新页面就丢，而邀请本身活 24 小时且跨刷新存活。
  const remaining = expiresAt === null ? null : remainingSeconds(expiresAt, now);

  // 优先级：已用 > 过期 > 撤销 > 不可达。
  //
  // **`expired` 必须排在 `revoked` 前面**：注册表只保留「未过期且未撤销」的条目，所以
  // 「查不到」这一个现象对应两种成因。先用邀请自带的失效时刻把过期摘出来，剩下的才是
  // 撤销——反过来排会对着一条自然到期的邀请断言「你撤销过它」，那是可被用户证伪的假话。
  //
  // 「不可达」垫底：前三者是这条邀请自己已成定局的事实，而它描述的是本机此刻的状态。
  const overlay: OverlayKind | null =
    invite === null ? null
    : consumed ? "consumed"
    : remaining !== null && remaining <= 0 ? "expired"
    : revoked ? "revoked"
    : !reachable ? "unreachable"
    : null;

  return (
    /*
     * **横排与否看容器宽度，不看视口宽度。**
     *
     * 这里原本是 `sm:flex-row`（视口 ≥640px 就横排），但这块的实际归宿是设备页 ≥1280 档
     * 那条 **360px 侧栏**——视口 1440px 满足 `sm:`，容器却只有 320px（实测；360 减去
     * `--space-panel` 两侧各 20）：二维码固定占 220，右列剩下不到 100px，那段两行说明
     * 被切成每行三四个字。视口断点在这里表达的是「页面有多宽」，而这块要问的是「我有多宽」。
     *
     * `@md`（28rem = 448px）是二维码 220 + 说明列至少 ~180 的和：低于它就竖排，
     * 二维码居中、文字占满整行。
     */
    <div className="@container mt-3">
      <div className="flex flex-col items-center gap-3 @md:flex-row @md:items-start">
        <div
          className="relative flex shrink-0 items-center justify-center rounded-xl bg-white p-3 ring-1 ring-slate-900/[0.06] dark:ring-white/15"
          style={{ width: QR_SIZE + CARD_PADDING * 2, height: QR_SIZE + CARD_PADDING * 2 }}
        >
          {html ? (
            <div
              role="img"
              aria-label={t`配对邀请二维码`}
              // 压到扫不动为止：失效的码若还能被读到，对方只会白跑一趟失败流程。
              className={`size-full [&>svg]:size-full ${overlay ? "opacity-[0.09]" : ""}`}
              // SVG 由 wasm 侧受信任生成（纯几何 path，无脚本），内联安全。
              dangerouslySetInnerHTML={html}
            />
          ) : (
            // 白卡内固定深色，理由见文件头
            <p className="px-2 text-center text-xs text-slate-500">
              {invite === null ? <Trans>正在生成…</Trans> : <Trans>二维码生成失败，请用下方链接配对</Trans>}
            </p>
          )}
          {overlay && <QrOverlay kind={overlay} />}
        </div>

        <div className="flex w-full min-w-0 flex-col gap-2">
          {/* 扫码只有手机端做得到（桌面端无相机，见 `_app/pairing/input.lazy.tsx`），
              所以两条路径要分端说清楚，不能笼统写「扫码或粘贴」。 */}
          <p className="text-xs text-muted-foreground">
            <Trans>手机上用 SwarmDrop 的「扫码」对准它；桌面端没有相机，复制链接发过去用「粘贴邀请」。</Trans>
          </p>
          {/* 剩余有效期贴着说明走，不写死「24 小时内有效」——那句话在码已经躺了半天之后
              仍然照说不误，等于没说。失效后交给码面上的覆盖层，这里不再重复。 */}
          {overlay === null && expiresAt !== null && (
            <p className="text-xs text-fd-muted-foreground" aria-live="polite">
              {t(remainingLabel(expiresAt, now, i18n.locale))}
            </p>
          )}
          {/* `key` 让按钮随邀请换代重挂：否则「已复制」会在点了「重新生成」之后继续挂着，
              而剪贴板里躺的是刚被撤销的上一条；「复制失败」同理会残留到一条全新的邀请上。 */}
          <CopyInviteButton key={invite ?? "pending"} invite={invite} />
          {/* 手选兜底：复制按钮在极少数环境下不可用（权限被拒），链接本身仍要够得着。
              一行截断而不是多行 textarea——它是第三顺位的路径，不该占主要视觉重量。 */}
          <input
            readOnly
            value={invite ?? ""}
            aria-label={t`邀请链接`}
            onFocus={(e) => e.currentTarget.select()}
            className="w-full truncate rounded-lg border border-fd-border bg-fd-background px-2 py-1.5 font-mono text-xs text-fd-muted-foreground"
          />
        </div>
      </div>
    </div>
  );
});

const OVERLAY = {
  consumed: {
    icon: CheckCircle2,
    title: msg`邀请已被使用`,
    hint: msg`一条邀请只能用一次。要再连一台设备，请重新生成。`,
  },
  expired: { icon: TimerOff, title: msg`邀请已过期`, hint: msg`有效期已过，重新生成一条即可。` },
  revoked: { icon: Ban, title: msg`邀请已撤销`, hint: msg`这个码已经作废，请重新生成一条。` },
  unreachable: {
    // 只说「已撤销」不说「已断开」：这个分支唯一的成因是用户在设置页主动撤销了可达，
    // 而 store 里的 `reservation` 目前**感知不到真实断线**（helper 掉线不会把它置回 null），
    // 所以不能拿它当断线检测来承诺。
    icon: WifiOff,
    title: msg`本机可达已撤销`,
    hint: msg`circuit 已撤销，对方扫这个码也拨不回来。重新建立可达后请重新生成。`,
  },
} satisfies Record<
  OverlayKind,
  { icon: LucideIcon; title: MessageDescriptor; hint: MessageDescriptor }
>;

/**
 * 码面覆盖层。白底之上，所以文字用固定的 `slate` 深色而非主题 token（见文件头）。
 * 不带动作按钮——恢复动作只有一个「重新生成」，它就在码上方几十像素处，
 * 在这里再放一个等于同一屏出现两个同义主动作。
 */
function QrOverlay({ kind }: { kind: OverlayKind }) {
  const { t } = useLingui();
  const { icon: Icon, title, hint } = OVERLAY[kind];
  return (
    <div
      role="status"
      className="absolute inset-3 flex flex-col items-center justify-center gap-1.5 rounded-lg bg-white/85 px-3 text-center"
    >
      <Icon className="size-6 text-slate-400" aria-hidden="true" />
      <p className="text-xs font-medium text-slate-700">{t(title)}</p>
      <p className="text-[11px] leading-snug text-slate-500">{t(hint)}</p>
    </div>
  );
}

function CopyInviteButton({ invite }: { invite: string | null }) {
  // 结果就地显示在按钮上（应用区没有 toast 系统），成败两态的取舍见 hook 的注释。
  const { state, copy } = useCopyToClipboard();

  return (
    <button
      type="button"
      onClick={() => invite !== null && void copy(invite)}
      disabled={invite === null}
      className="inline-flex w-fit items-center gap-1.5 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
    >
      {state === "copied" ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
      {state === "copied" ? (
        <Trans>已复制</Trans>
      ) : state === "failed" ? (
        <Trans>复制失败，请手动选中</Trans>
      ) : (
        <Trans>复制链接</Trans>
      )}
    </button>
  );
}
