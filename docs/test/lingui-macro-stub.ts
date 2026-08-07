/**
 * `@lingui/core/macro` 的**测试替身**。
 *
 * `msg\`\`` 是编译期宏，由 Next 的 SWC 插件（`@lingui/swc-plugin`，见 next.config.mjs）展开。
 * vitest 走 esbuild，不吃那条管线，于是任何 import 了 `_lib/*.ts` 的测试都会在模块加载时炸
 * （`Cannot find package 'babel-plugin-macros'`）——而 `_lib` 下四个文件都用它存标签描述符
 * （`nav.ts` / `view-types.ts` / `store.ts` / `invite.ts`）。
 *
 * 替身产出与宏一致的形状（`{ id, message }`），足以让读这些映射表的代码在测试里跑起来。
 * **只影响测试**：产物与 dev server 仍走真正的 SWC 宏展开。
 *
 * 不装 `@lingui/babel-plugin-lingui-macro` 的理由：那会把 babel 拉进测试管线，为了几个纯函数
 * 测试换来一条与生产完全不同的转译路径。真要测**渲染出的译文**时才值得——那时按本文件同级的
 * `vitest.config.ts` 头注释走 babel 插件那条路。
 */

/** 与宏展开后的 `MessageDescriptor` 同形。 */
export function msg(strings: TemplateStringsArray, ...values: unknown[]): {
  id: string;
  message: string;
} {
  const message = strings.reduce(
    (acc, part, i) => acc + part + (i < values.length ? `{${i}}` : ""),
    "",
  );
  return { id: message, message };
}

/** `defineMessage` 是 `msg` 的别名形式，一并给出以免下一个调用点又炸一次。 */
export const defineMessage = msg;
