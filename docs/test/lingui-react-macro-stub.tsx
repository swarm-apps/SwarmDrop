import type { ReactNode } from "react";

/**
 * docs 的组件测试只验证交互状态，不复刻 Next 的 Lingui SWC 转译；生产构建仍使用真实宏。
 */
export function Trans({ children }: { children?: ReactNode }) {
  return <>{children}</>;
}

export function useLingui() {
  return {
    t(strings: TemplateStringsArray, ...values: unknown[]) {
      return strings.reduce(
        (message, part, index) => message + part + (index < values.length ? String(values[index]) : ""),
        "",
      );
    },
  };
}
