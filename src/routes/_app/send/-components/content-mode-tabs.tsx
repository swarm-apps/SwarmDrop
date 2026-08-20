/**
 * 发送内容类型切换（文件 / 文本）。
 *
 * **它挂在页头（`TaskToolbar` 的 `trailing`）而不是内容区上方独占一行。** 此前是一条
 * 224×44 的分段控件横跨整个 `max-w-[1180px]` 内容带、只用掉其中 19% 的宽度——尺寸与
 * `CommandDock` 里的主操作按钮同级，而 `DESIGN.md` 的 Content Mode Selector 写的是
 * 「紧凑分段控件，不是常驻导航项」。挪进页头省掉整整 60px（正好是文件列表少一行的高度），
 * 页头右侧那片空白也一并被填上。
 *
 * **为什么不是设备摘要条的右侧**（那是最初的提法）：那一格已经被「已选内容」计量占着，
 * 而设备名最长 40 个中文字（`DeviceName::MAX_CHARS`），弹性空间不可预测。移动端与 Web
 * 端没有这个冲突（移动端的计量在底部操作栏、Web 端那行右侧只有一个 ghost 按钮），
 * 所以那两端确实落在设备行上——三端落点不同但语义相同，判据在 DESIGN.md。
 *
 * **走 shadcn 的 `Tabs`（Radix）而不是自己拼一组按钮。** roving tabindex、方向键、
 * Home/End、`aria-controls` / `aria-labelledby` 的自动关联全是 Radix 已经做对的事；
 * 手写一遍只会得到一个少几种键盘行为的复制品——`reference/product.md` 把「为口味重新
 * 发明标准 affordance」列为 product register 的禁项。尺寸也用默认的：这是 shadcn 分段
 * 控件在本仓的第一个使用者，没有理由先给它开一套例外。
 *
 * 值与切换由外层 `<Tabs value=… onValueChange=…>` 持有（页面还要拿 mode 去驱动摘要条、
 * 底部按钮与拖放开关），所以这里只认 `disabled`。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { FileText, type LucideIcon, Type } from "lucide-react";
import { TabsList, TabsTrigger } from "@/components/ui/tabs";

export type SendContentMode = "files" | "text";

export function ContentModeTabs({ disabled }: { disabled?: boolean }) {
  const { t } = useLingui();
  /**
   * 图标三端必须同一套（知识库「三端同一概念必须同一个图标」）。此前是三套：
   * 桌面 `FileText`/`Type`、移动 `FileText`/`Clipboard`、Web `Paperclip`/`Clipboard`。
   * 统一取 `FileText` + `Type`——`Clipboard` 说的是「从剪贴板来」，而这一档同样接受手打
   * 的文本，剪贴板只是其中一个入口（面板里那个「从剪贴板粘贴」按钮才是它）。
   */
  const modes: readonly {
    value: SendContentMode;
    icon: LucideIcon;
    label: React.ReactNode;
  }[] = [
    { value: "files", icon: FileText, label: <Trans>文件</Trans> },
    { value: "text", icon: Type, label: <Trans>文本</Trans> },
  ];

  return (
    <TabsList aria-label={t`发送内容类型`}>
      {modes.map(({ value, icon: Icon, label }) => (
        <TabsTrigger
          key={value}
          value={value}
          disabled={disabled}
          data-testid={`send-content-mode-${value}`}
        >
          <Icon aria-hidden />
          {label}
        </TabsTrigger>
      ))}
    </TabsList>
  );
}
