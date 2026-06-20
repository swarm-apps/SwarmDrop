import type { ReactNode } from "react";
import { ScrollView } from "react-native";
import { Text } from "@/components/ui/text";

export interface ReleaseNotesViewProps {
  notes?: string;
  /** 自定义渲染(如接 Markdown 渲染器);缺省按纯文本渲染(保留换行)。 */
  renderer?: (notes: string) => ReactNode;
  /** 覆盖容器最大高度,默认 220。 */
  maxHeight?: number;
}

export function ReleaseNotesView({
  notes,
  renderer,
  maxHeight = 220,
}: ReleaseNotesViewProps) {
  if (!notes) return null;
  return (
    <ScrollView
      style={{ maxHeight }}
      contentContainerClassName="pr-1"
      showsVerticalScrollIndicator
    >
      {renderer ? (
        renderer(notes)
      ) : (
        <Text className="text-muted-foreground text-sm leading-5">{notes}</Text>
      )}
    </ScrollView>
  );
}
