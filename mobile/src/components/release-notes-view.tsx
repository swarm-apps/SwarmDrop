// release-notes-view —— release notes 渲染槽(NativeWind + RNR Text)。缺省用
// react-native-marked 渲染 Markdown(标题/加粗/列表/链接/代码均按真实 markdown 呈现),
// 主题色从 NativeWind CSS 变量读取(global.css 唯一真相源,随明暗自动切)。可通过 `renderer`
// 覆盖为自定义渲染器。背景 / 圆角由各父组件的 bg-muted 盒子负责,本组件不带底色。
// registry:component。dependency: react-native-marked(peer: react-native-svg)。

import { useUnstableNativeVariable } from "nativewind";
import { type ReactNode, useMemo } from "react";
import { ScrollView, useColorScheme } from "react-native";
import { useMarkdown } from "react-native-marked";

export interface ReleaseNotesViewProps {
  notes?: string;
  /** 自定义渲染(如接自定义 Markdown 渲染器);缺省用 react-native-marked 渲染 Markdown。 */
  renderer?: (notes: string) => ReactNode;
  /** 覆盖容器最大高度,默认 220。 */
  maxHeight?: number;
}

const hsl = (v?: string | null) => (v ? `hsl(${v})` : undefined);

function headingStyle(fontSize: number, color?: string) {
  return {
    fontSize,
    lineHeight: fontSize + 5,
    fontWeight: "600" as const,
    color,
    marginTop: 6,
    marginBottom: 2,
  };
}

export function ReleaseNotesView({
  notes,
  renderer,
  maxHeight = 220,
}: ReleaseNotesViewProps) {
  const colorScheme = useColorScheme();
  // 主题色来自 NativeWind CSS 变量(global.css 唯一真相源),随明暗自动切换。
  const mutedForeground = hsl(
    useUnstableNativeVariable("--muted-foreground") as string | undefined,
  );
  const foreground = hsl(
    useUnstableNativeVariable("--foreground") as string | undefined,
  );
  const primary = hsl(
    useUnstableNativeVariable("--primary") as string | undefined,
  );
  const muted = hsl(useUnstableNativeVariable("--muted") as string | undefined);
  const border = hsl(
    useUnstableNativeVariable("--border") as string | undefined,
  );

  const theme = useMemo(
    () => ({
      colors: {
        text: mutedForeground ?? "#64748b",
        link: primary ?? "#087968",
        code: muted ?? "#f1f5f9",
        border: border ?? "#e2e8f0",
      },
      spacing: { xs: 2, s: 4, m: 8, l: 12 },
    }),
    [mutedForeground, primary, muted, border],
  );

  const styles = useMemo(
    () => ({
      text: { fontSize: 13, lineHeight: 20, color: mutedForeground },
      paragraph: { marginTop: 0, marginBottom: 0, paddingVertical: 2 },
      strong: { fontWeight: "600" as const, color: foreground },
      em: { color: mutedForeground },
      h1: headingStyle(15, foreground),
      h2: headingStyle(15, foreground),
      h3: headingStyle(13, foreground),
      h4: headingStyle(13, foreground),
      h5: headingStyle(13, foreground),
      h6: headingStyle(13, foreground),
      li: { fontSize: 13, lineHeight: 20, color: mutedForeground },
      link: { color: primary },
      codespan: { fontSize: 12, color: foreground },
    }),
    [mutedForeground, foreground, primary],
  );

  // hook 必须无条件调用:notes 为空时传空串,渲染层再决定是否显示。
  const elements = useMarkdown(notes ?? "", { colorScheme, theme, styles });

  if (!notes) return null;

  return (
    // flexGrow:0 是关键:RN 的 ScrollView 隐式 flexGrow>0,放在自适应容器里会填满 maxHeight,
    // 短内容也占满一大块空盒、把弹窗 footer 挤出卡片。显式置 0 后即「按内容自适应、超出才滚」
    // (等价 web 的 max-height + overflow-auto)。
    <ScrollView
      style={{ maxHeight, flexGrow: 0 }}
      showsVerticalScrollIndicator
      nestedScrollEnabled
      contentContainerClassName="pr-1"
    >
      {renderer ? renderer(notes) : elements}
    </ScrollView>
  );
}
