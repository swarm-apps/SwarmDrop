import { useSafeAreaInsets } from "react-native-safe-area-context";

/**
 * 底部固定区的**视觉呼吸位**（dp），与底栏 `pt-3` 对称。
 *
 * 单独导出而不是只当默认参数，是因为**不吃安全区的那一档要用同一个数**：
 * `transfer-offer-host` 的平板浮层底缘不碰系统条，只需要呼吸位。两边各写一个 `12`
 * 正是「同一件事五个魔数」的来源（见仓库根的
 * `dev-notes/research/2026-08-10-mobile-bottom-action-audit.md` —— 注意是**仓库根**那份
 * `dev-notes/`，不是 `mobile/dev-notes/`，后者没有 `research/`）。
 */
export const BOTTOM_BREATHING = 12;

/**
 * 底部固定区（动作栏 / sheet footer）的 `paddingBottom`
 * = **系统占用**（Android 手势条 / 三键导航、iOS home indicator）**+ 视觉呼吸位**。
 *
 * 两者**相加**，不是 `Math.max`。取大在所有现代设备上恒等于 `insets.bottom`
 * ——Android 手势条 24dp、三键 48dp，iOS home indicator 34dp，全都 ≥ 呼吸位——
 * 于是主按钮与系统条之间**永远零间距**，看上去就是「贴着屏幕底缘」。
 *
 * ## 它收口了什么、没收口什么
 *
 * **不是全仓所有底距的唯一出口**，只负责「自己坐在屏幕底缘、必须自行避开系统条」的那一类。
 * 当前调用点三处：
 * - `components/mobile/screen.tsx` 的 `BottomActionBar`（覆盖 7 个 stack 屏，含
 *   `device/groups` 那条包在 `KeyboardStickyView` 里的新建栏）；
 * - `components/transfer-offer-host.tsx` 的 `OfferActions`（手机走 sheet → 吃 inset；
 *   平板浮层底缘不碰系统条 → 只用 {@link BOTTOM_BREATHING}）；
 * - `app/device/[peerId].tsx` 的 `PolicyActionFooter`（策略 sheet 的 footer，底边即屏底）。
 *
 * 下面几类**故意不走这里**，别顺手改：
 * - `components/onboarding/onboarding-scaffold.tsx` 与三个配对页
 *   （`app/pairing/{scan,found-device,success}.tsx`）：它们用 `SafeAreaView edges` 含
 *   `"bottom"` 把 inset 加在**外层容器**上，footer 自己的 `pb-4` / `pb-6` 才是呼吸位。
 *   公式同样是相加、语义已经对了——本 hook 的「相加不取大」正是**照它们写的**。
 *   换成 hook 等于换机制（要先摘掉 SafeAreaView 的 bottom edge），且这两处的呼吸位是
 *   16 / 24 而非 12（引导与配对是全屏单 CTA 的松版式），不是同一个数。
 * - `app/(main)/index.tsx` 的 `HomeDock`：tab 屏，iOS 的 inset 由 `AppScreen` 的
 *   iOS 专属 bottom edge 提供，Android 由实体 NativeTabs 栏兜住——它自己吃就双垫了。
 * - 各 sheet 的**内容**底距（`inbox/[itemId]` 的 `pb-6`、添加设备 sheet 的 `pb-8`、
 *   `device-organization-sheets` 的 `pb-6`）：那是内容收尾留白，且 `AppBottomSheet`
 *   整体不吃安全区。正解是在 `AppBottomSheet` 里**吃一次**（audit 的 Phase F），
 *   不是让每个 sheet 调用点各调一次本 hook。
 * - 列表的 `contentContainerStyle.paddingBottom`（`LIST_CONTENT_PADDING` 的 32、
 *   `device/groups` 与 file-browser 的 24）：滚动内容的收尾留白，与系统条无关。
 *
 * @param breathing 视觉呼吸位（dp）。默认 {@link BOTTOM_BREATHING}。
 */
export function useBottomSafePadding(breathing = BOTTOM_BREATHING) {
  return useSafeAreaInsets().bottom + breathing;
}
