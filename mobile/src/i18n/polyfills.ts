/**
 * Intl polyfill —— 必须在 `@lingui/core` 被使用前执行。
 *
 * Hermes 的 Intl 只有 `getCanonicalLocales / Collator / DateTimeFormat / NumberFormat`
 * 四样(真机实测 `Object.getOwnPropertyNames(Intl)` 的原话),**既没有 PluralRules,
 * 也没有 Locale**。而 lingui 的复数格式化无条件 `new Intl.PluralRules(...)` 且不 catch,
 * 于是任何 `<Plural>` / ICU plural 消息在渲染期直接抛 TypeError —— release 下即闪退。
 * (踩过:`src/app/device/groups.tsx` 分组行的设备数,设备数 ≥ 1 时必崩。)
 *
 * 两个 polyfill 缺一不可,且**顺序不能反**:intl-pluralrules 内部做 locale 匹配时会
 * `new Intl.Locale(...)`,只补 PluralRules 会在 @formatjs/intl-localematcher 里换个地方崩。
 *
 * 用 `polyfill-force` 而非 `polyfill`:后者带 native 探测,在 Android 上会显著拖慢启动,
 * 而这里缺失是确定性的(Hermes 就是没有),不需要探测。
 *
 * locale-data 必须同时载 zh + en:lingui 的 normalizeLocales 会无条件追加 "en" 兜底,
 * 实际解析的 locales 恒为 [当前语言, "en"],少载任一个都可能抛 RangeError。
 *
 * 后缀 .js 不能省:这两个包的 exports map 只暴露带 .js 的路径,省掉解析不到。
 */

import "@formatjs/intl-locale/polyfill-force.js";
import "@formatjs/intl-pluralrules/polyfill-force.js";
import "@formatjs/intl-pluralrules/locale-data/zh.js";
import "@formatjs/intl-pluralrules/locale-data/en.js";
