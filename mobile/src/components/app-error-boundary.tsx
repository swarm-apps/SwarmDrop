import { msg } from "@lingui/core/macro";
import * as Clipboard from "expo-clipboard";
import { type ErrorBoundaryProps, router } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { useCallback, useEffect, useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native";
import { i18n } from "@/i18n/lingui";
import { errorDetail } from "@/lib/errors";

/**
 * 全 App 的 JS 异常兜底屏:把「任意一处没想到的 JS 异常 = 整个 App 闪退」收敛成一屏可读的
 * 错误 + 出口。**它不修 bug,只收爆炸半径** —— 别因为有它就容忍抛异常的正常路径。
 *
 * **改本文件前必读** `knowledge/toolchain.md` 的「ErrorBoundary 是兜底」:那里有两起事故的
 * 复盘、挂载点的选择理由(尤其是为什么绝不能挂 `(main)/_layout.tsx`),以及下面三条约束的推导。
 *
 * **三条硬约束**(它要在最坏情况——根 layout 自己抛错——下还能画出来):
 * ① 文案走全局 `i18n._()`,不用 `<Trans>`/`useLingui()`(那要 Provider,而 boundary 被包在
 *    layout 外面);② 不碰 SafeAreaView 与 `@/components/ui/*`;③ 文案不含复数/ICU
 *    (`plural()` 无条件 `new Intl.PluralRules`,正是其中一起事故的崩因)。
 *
 * 已知边界:根 layout 首次渲染就抛时 `initI18n()` 还没跑过,`_()` 回落 zh-Hans 源串,
 * 英文用户会看到中文标题。没加双语兜底 —— 那种场景 App 根本没起来,而详情与 stack 本就
 * 语言无关,截图反馈照样成立;把 boundary 搞复杂反而更容易连它一起崩。
 */

const TITLE = msg`这个页面出错了`;
const DESCRIPTION = msg`已经停在这里，你的传输记录和收到的文件都没有受影响。`;
const RETRY = msg`重试`;
const GO_BACK = msg`返回上一页`;
const COPY = msg`复制详情`;
const COPIED = msg`已复制`;
const DETAIL_LABEL = msg`错误详情`;

export function AppErrorBoundary({
  error,
  retry,
  title,
  description,
}: ErrorBoundaryProps & {
  /**
   * 覆盖标题。给**启动失败**用(`app/_layout.tsx` 的 boot 分支复用本组件的外壳):
   * 那时说「这个页面出错了」不准确——不是某个页面,是整个 App 没起来。
   * 其余情况别传,用默认的。
   */
  title?: string;
  /**
   * 覆盖正文。同样给启动失败用,两个原因:默认那句「你的传输记录和收到的文件都没有受影响」
   * 是**页面级**的安抚,启动都没成功时说它没意义;而且 boot 那条路径该展示
   * `getErrorMessage(err)` 的本地化文案——下面的详情框里是 `errorDetail`,
   * 那是 Rust 侧写的中文技术串,`lib/errors.ts` 明确规定它不进 UI 主文案。
   */
  description?: string;
}) {
  const [copied, setCopied] = useState(false);
  // `error` 在类型上是必填,但这屏的存在意义就是接住意外 —— 真拿到 undefined 时
  // `errorDetail` 会返回字符串 "undefined",而这恰好是让用户抄给我们的那一段。
  const detail = error ? errorDetail(error) : "(no error object)";
  const onCopy = useCallback(() => {
    void Clipboard.setStringAsync(
      error?.stack ? `${detail}\n\n${error.stack}` : detail,
    );
    setCopied(true);
    // 复位,否则按钮永远停在「已复制」—— 用户第二次点(比如剪贴板被粘贴覆盖后)看不到任何反馈。
    setTimeout(() => setCopied(false), 2000);
  }, [detail, error]);

  // 根 boundary 触发时整棵树被替换,此时栈里未必还有上一页;没有就只渲染「重试」。
  const canGoBack = router.canGoBack();

  // expo-router 自己也会 log,但那条在 release 的 logcat 里容易被淹没;这里再落一份带前缀的。
  useEffect(() => {
    console.error("[error-boundary]", errorDetail(error), error?.stack);
    // **必须自己收启动屏**:`preventAutoHideAsync()` 在 `_layout.tsx` 模块作用域就调了,
    // 而唯一的 `hideAsync()` 在 boot effect 的 finally 里。RootLayout 首次渲染就抛时
    // 那个 effect 永远不跑 —— 本屏会被原生启动图整个盖住,用户看到的是"卡在启动画面",
    // 连重试按钮都摸不到。而"根 layout 抛错"恰恰是本组件存在的头号理由。
    SplashScreen.hideAsync().catch(() => {});
  }, [error]);

  return (
    <View className="flex-1 justify-center gap-5 bg-background px-6 py-16">
      <View className="gap-2">
        <Text className="text-[20px] font-semibold text-foreground">
          {title ?? i18n._(TITLE)}
        </Text>
        <Text className="text-[14px] leading-6 text-muted-foreground">
          {description ?? i18n._(DESCRIPTION)}
        </Text>
      </View>

      <View className="gap-2">
        <View className="flex-row items-center justify-between gap-3">
          <Text className="text-[12px] text-muted-foreground">
            {i18n._(DETAIL_LABEL)}
          </Text>
          {/* 让用户能把这段交出来,而不是逼他截一屏 minify 过的 stack 再手打。 */}
          <Pressable
            onPress={onCopy}
            accessibilityRole="button"
            testID="app-error-boundary-copy"
            className="min-h-9 justify-center rounded-lg border border-border px-3 active:opacity-70"
          >
            <Text className="text-[12px] font-medium text-foreground">
              {i18n._(copied ? COPIED : COPY)}
            </Text>
          </Pressable>
        </View>
        <ScrollView
          className="max-h-48 rounded-lg border border-border bg-card"
          contentContainerClassName="p-3"
        >
          <Text className="font-mono text-[12px] leading-5 text-foreground">
            {detail}
          </Text>
          {/* stack **不按 __DEV__ 门控**:dev 有 LogBox 和 Metro symbolication,压根不需要它;
              release 才是这屏唯一起作用的地方,而那里 `error.message` 常常只是
              「undefined is not a function」——没有帧、没有符号,截图给上来等于没给。
              minify 后的帧至少还有函数名与行号,配 sourcemap 能还原。 */}
          {error?.stack ? (
            <Text className="mt-2 font-mono text-[11px] leading-4 text-muted-foreground">
              {error.stack}
            </Text>
          ) : null}
        </ScrollView>
      </View>

      {/* 两个出口。只给「重试」的话,确定性失败(比如某条坏记录必然让详情页抛)会原地循环,
          用户走不掉 —— 而根 boundary 一触发就是整棵树被换掉,连返回手势都没了。
          `router` 用命令式单例,不吃 context,不破坏上面那三条约束。 */}
      <View className="flex-row gap-3">
        {canGoBack ? (
          <Pressable
            onPress={() => router.back()}
            accessibilityRole="button"
            testID="app-error-boundary-back"
            className="min-h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
          >
            <Text className="text-[14px] font-semibold text-foreground">
              {i18n._(GO_BACK)}
            </Text>
          </Pressable>
        ) : null}
        <Pressable
          onPress={() => void retry()}
          accessibilityRole="button"
          testID="app-error-boundary-retry"
          className="min-h-12 flex-1 items-center justify-center rounded-xl bg-primary active:opacity-70"
        >
          <Text className="text-[14px] font-semibold text-primary-foreground">
            {i18n._(RETRY)}
          </Text>
        </Pressable>
      </View>
    </View>
  );
}
