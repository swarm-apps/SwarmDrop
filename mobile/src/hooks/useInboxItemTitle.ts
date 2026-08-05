import { useLingui } from "@lingui/react/macro";

/**
 * 收件箱条目的展示标题：内核只给结构（首文件名 + 文件数），拼串是呈现边缘的事。
 *
 * 此前这条串由 Rust 侧的 `inbox_title` 拼好并**落库**，于是切语言时存量条目纹丝不动，
 * Lingui 也完全够不着它。判据与三端各写一份的理由见桌面端
 * `src/routes/_app/inbox/index.lazy.tsx` 的 `inboxItemTitle`。
 *
 * 写成 hook 是因为 `t` 要从 `useLingui` 拿——切语言时才会重新求值。
 *
 * 入参是 `string | undefined` 而不是桌面 / Web 的 `string | null`：uniffi 把 Rust 的
 * `Option<String>` 生成成**可选属性**（`primaryFileName?: string`），照抄桌面签名会
 * 直接 TS2345。两套 codegen 的映射约定不同，不是可以统一掉的东西
 * （记在 `mobile/dev-notes/knowledge/rust-bridge.md`）。
 */
export function useInboxItemTitle() {
  const { t } = useLingui();
  // 不包 useCallback：三个调用点都是渲染期内联调用，既不进依赖数组也不作 prop。
  return (primaryFileName: string | undefined, itemCount: number) => {
    if (!primaryFileName) return t`空传输`;
    if (itemCount <= 1) return primaryFileName;
    return t`${primaryFileName} 等 ${itemCount} 个文件`;
  };
}
