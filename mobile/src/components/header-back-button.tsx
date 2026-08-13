import { useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ChevronLeft } from "lucide-react-native";
import { HeaderIconButton } from "@/components/mobile/screen";

/**
 * 导航条的返回入口 —— `SettingsHeader` 与 `SearchHeader` 共用这一份。
 *
 * 抽出来是因为两边曾经各写各的:一边裸 22px `ArrowLeft` + `hitSlop`,一边
 * `HeaderIconButton` + `ChevronLeft`,于是**图标、形态、光心位置、触摸目标四样全不一样**,
 * 从传输记录 push 到搜索页时返回入口会当场换个样子。定义只留一处就不会再漂。
 *
 * 形态选择的依据(2026-08-08 调研后定):
 * - `HeaderIconButton` 是 `DESIGN.md` 的 Radius Vocabulary Rule **点名列举**的「图标方钮」
 *   (`rounded-xl`),裸箭头不属于该规则的任何一类,是规范外的漂移;
 * - 44px 是**看得见**的实体触摸目标。裸图标靠 `hitSlop` 把 22px 撑到 46px,够大但不可见,
 *   用户不知道能点哪(Material 3 要求导航图标 ≥48dp,iOS HIG 要求 ≥44pt);
 * - `ChevronLeft` 是识别率最高的返回图标(iOS 默认、业界标准),`ArrowLeft` 是 Android 传统
 *   ——自绘 header 跨两端只该有一套;
 * - 与右侧操作按钮同形,同一条导航条内左右视觉重量才均衡(三个带操作的页此前是「左裸右实」)。
 */
export function HeaderBackButton({
  onPress,
  disabled,
  testID,
}: {
  /**
   * 覆盖返回动作。默认 `router.back()`;配对确认这类「返回前要先撤销待决状态」的场景
   * 传自己的处理器,而不是另画一个按钮。
   */
  onPress?: () => void;
  /** 进行中不让退(如配对正在确认)。 */
  disabled?: boolean;
  testID?: string;
}) {
  const router = useRouter();
  const { t } = useLingui();
  return (
    <HeaderIconButton
      icon={ChevronLeft}
      label={t`返回`}
      onPress={onPress ?? (() => router.back())}
      disabled={disabled}
      testID={testID}
    />
  );
}
