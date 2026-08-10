我逐条核实了两份报告，并对两张截图做了像素测量。下面是核实结果与统一方案。

---

# 一、抽查结果：两份报告的对与错

## 一致且我亲自证实的

| 结论 | 证据 |
|---|---|
| 设备详情**压根没有滚动容器** | `mobile/src/app/device/[peerId].tsx:287-292` 调 `AppScreen` 时无 `scroll`；`screen.tsx:113` 的三元 falsy 分支是 `screen.tsx:130` 的纯 `<View className="flex-1 …">`；`[peerId].tsx:25` 的 RN import 只有 `ActivityIndicator, Pressable, View`，全文件零 `ScrollView/FlatList` |
| `BottomActionArea` 无背景、无安全区 | `screen.tsx:364` = `"gap-2 border-t border-border px-5 py-4"`，函数体（`screen.tsx:356-368`）不含 `useSafeAreaInsets` |
| `BottomActionBar` 才是详情页那个 | `screen.tsx:384-385`：`bg-background` + `paddingBottom: Math.max(insets.bottom, 12)` |
| 全仓只有 2 个 `BottomActionArea` 调用点 | `(main)/index.tsx:485`（tab 屏 `footer` 槽，正确）与 `device/[peerId].tsx:424`（错） |
| `AppBottomSheet` 完全不碰安全区 | `src/components/ui/app-bottom-sheet.tsx` 全文零 `useSafeAreaInsets` / `bottomInset` |
| `DESIGN.md` / `PRODUCT.md` **没有**底部操作区或安全区契约 | `rg -ni "safe.?area\|bottom action\|底部操作\|home indicator\|手势条"` 只命中 `DESIGN.md:381/678/800` 三处无关文本 |

## 分歧一：屏底那 99px 死带是什么 —— **B 对，A 错**

A 猜是「Android 导航栏 inset 已被屏容器吃掉」。**不是。** 我在 `1000026094.jpg`（1260×2800，density 3.0，用 header 返回键 `size-11`=44dp→实测 133px 标定）逐像素量：

```
x=30（页面左边距，所有卡片之外）：
  y2454-2456  #DFEAE6   ← BottomActionArea 的 border-t
  y2458-2799  #FFFFFF   ← 一路到屏底全是页面背景
x=300（穿过第二张卡）：
  y2458-2504  #EFF5F3   ← bg-muted，从透明底栏背后穿出来
  y2507-2652  #408D7B   ← 发送按钮（146px ≈ 48.7dp = min-h-12）
  y2654-2757  #EFF5F3 + #687480 字形  ← 「保存位置 / 跟随默认接收位置」
  y2758-2797  卡片顶边被屏底切断
```

底栏底边 = 2653 + `pb-4`(48px) = **y≈2701**，屏底 2800 → 空 99px。`gap-4` = 16dp = 48px，两个 `ConfirmDialog`（`[peerId].tsx:484`、`:498`）各撑出一个 → 96px。**与实测 99px 吻合**，且这个机制被本仓自己的注释写死过：

```
mobile/src/app/settings/bootstrap-nodes.tsx:206-207
// ConfirmDialog 放在 AppScreen **外面**:它的 Root 会渲染一个真实的零高 View,
// 留在 `gap-3` 的滚动内容里会让页面末尾凭空多出 12px 死带(Yoga 的 gap 不看子节点高度)。
```

`ConfirmDialog` → `AlertDialog` = `AlertDialogPrimitive.Root`（`ui/alert-dialog.tsx:12`），确实渲染真实零高 View；两个 gorhom sheet 走 Portal，在原地不产生节点，所以正好是 2 个 gap 而不是 4 个。

**这条分歧要紧**：A 由此推出「`Math.max(insets.bottom,12)` 可能会再吃一遍、双垫，先别动」——那个告诫建立在错误前提上。这一页 `edges` 是 `["top"]`（`screen.tsx:109`，无 `footer`、非 iOS），**整条链路没有任何一处吃过 bottom inset**，不存在双垫风险。

## 分歧二：两个发送页 —— **A 对，B 漏了**

B 对三个发送页的判定是「内容遮挡：无 / 背景：无缺陷 / 只有 `Math.max` 那个安全区缺陷」。**不成立。** `1000026055.jpg` 实测（同 1260×2800）：

```
x=30 （底栏外侧）: y2694-2697 #DFEAE6   ← BottomActionBar 的 border-t
y=2690 横切:  x61-265 #408D7B（进度条已填充）| x266-1198 #8DC1B7（未填充轨道）
              → 进度条 y2686-2703（18px = h-1.5 = 6dp），**整条压在 border-t 之上 8px**
y=2750 横切:  x62-1193 #96C5BD（= primary 50% 叠白 = disabled 按钮）
y=2799:       按钮颜色仍在 → 按钮被屏幕底缘直接切断，paddingBottom 一点没渲染出来
```

内容在底栏的 content box 上下**对称溢出**——这是 `flex-row items-center`（`screen.tsx:384`，交叉轴 center）遇上「子节点比 content box 高」的唯一签名。B 报告里那句「无缺陷」是没看这个状态（默认不复现，只在 prepare 期间出现）。

结构上的成因 A 说对了：`prepare-progress-bar.tsx:32` 的根节点是 `<View className="flex-1 gap-2">`，它落在 `share-target.tsx:235` / `select-device.tsx:260` 那个**纵向** `<View className="flex-1 gap-2">` 里。`flex-1` = `flexBasis: 0%`，在纵向容器里作用于**高度**；容器高度 auto 时基准尺寸 0、`flexGrow` 无空间可长，于是它塌了，而 `share-target.tsx:249` 的 Pressable 又同样带 `flex-1`（靠 `min-h-12` 才有 48dp）。测量到的形状与这个模型一致。

> 我没有真机复现，只有像素测量 + 结构推断。**修法本身是无风险的**（见下 §3 Phase B：那两个 `flex-1` 在纵向容器里本来就是误用，删掉不改变横向撑满——列容器默认 `align-items: stretch`），但要在真机 prepare 一个大文件确认。

## 分歧三：`BottomActionArea` 该补注释还是该删 —— 两份都保守了

A 建议给它加 JSDoc + `bg-background`。我不同意：**一个共享组件只有 1 个合法调用点、却已经造成一次功能不可达，它就不该是共享的**。见 §3 Phase D。

---

# 二、这是「三个页面各修各的」还是「一处统一修」？

**都不是。是三条独立根因，其中两条一改多受益。** 不需要抽新组件——`BottomActionBar` 已经是那个组件，它没坏。

| 根因 | 影响面 | 修在哪 |
|---|---|---|
| **R1** 用错组件（`BottomActionArea` 而非 `BottomActionBar`）+ 无滚动容器 | 只有 `device/[peerId]` 一页 | 页面级 |
| **R2** `flex-1` 用在纵向轴 → `PrepareProgressBar` 塌高 | 2 个发送页 | **共享组件一处**（`prepare-progress-bar.tsx:32`） |
| **R3** 底距公式 `Math.max(inset, 12)` 语义错（系统占用 vs 视觉呼吸位是**相加**不是取大） | 7 个 `BottomActionBar` 页 + 1 个 sheet | **共享组件一处**（`screen.tsx:385`） |

R3 值得展开：`Math.max(insets.bottom, 12)` 在**所有现代设备上恒等于 `insets.bottom`**（Android 手势条 24dp、三键 48dp、iOS home indicator 34dp，全都 ≥ 12）。也就是说主按钮与系统条之间的视觉间距**恒为 0**——这正是用户说的「完全贴屏幕底缘」。仓内已有的两种正确写法都是相加：`onboarding-scaffold.tsx:28+31`（SafeAreaView 吃 inset + `pb-4`）、三个配对页（`edges=["top","bottom"]` + `pb-6`）。

---

# 三、方案（按阻塞度分阶段）

## Phase A — 设备详情（S0，必须）

`mobile/src/app/device/[peerId].tsx`，四处改动：

```diff
@@ :25
-import { ActivityIndicator, Pressable, View } from "react-native";
+import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";

@@ :42-46
 import {
   AppScreen,
-  BottomActionArea,
+  BottomActionBar,
   Surface,
 } from "@/components/mobile/screen";

@@ :286-295  外层套 <> 把 ConfirmDialog 移出 AppScreen；内容 View → ScrollView
   return (
+    <>
     <AppScreen
       testID="device-detail-screen"
       header={<SettingsHeader title={t`设备详情`} />}
       bare
-      contentClassName="gap-4"
     >
-      {/* pt-4 是导航条与首张卡之间的间距。… */}
-      <View className="flex-1 gap-4 px-5 pt-4">
+      {/* pt-4 与列表页 LIST_CONTENT_PADDING_UNDER_HEADER 同值；pb-6 是滚到底时
+          最后一张卡与操作栏之间的呼吸位。操作栏是流内兄弟节点、不悬浮，
+          所以这里**不需要**补一个等于操作栏高度的 paddingBottom。 */}
+      <ScrollView
+        className="flex-1"
+        showsVerticalScrollIndicator={false}
+        contentContainerClassName="gap-4 px-5 pt-4 pb-6"
+      >
         …两张 Surface 原样不动…
-      </View>
+      </ScrollView>

@@ :424-451
-      <BottomActionArea>
+      <BottomActionBar testID="device-detail-action-bar">
         <Pressable
           …
-          className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
+          className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
         >
-      </BottomActionArea>
+      </BottomActionBar>

@@ :484-516  两个 ConfirmDialog 移到 </AppScreen> 之后
     </AppScreen>
+    <ConfirmDialog … unpair … />
+    <ConfirmDialog … block … />
+    </>
   );
```

三点说明：
- **`contentClassName="gap-4"` 必须删**。它作用在 `AppScreen` 的内容盒上（`screen.tsx:130`），不是卡片之间——卡片间距由 ScrollView 的 `contentContainerClassName` 里的 `gap-4` 提供。留着它就是那 96px 死带的来源之一。
- **`flex-1` 必须加在 Pressable 上**：`BottomActionBar` 是 `flex-row`（`screen.tsx:384`），单子节点不加会缩成文字宽度。写法与 `send/share-target.tsx:249`、`send/shared-files.tsx:52` 一致。
- 形态照抄 `mobile/src/app/inbox/[itemId].tsx:497-500`（同为 `AppScreen bare` + 显式 ScrollView + 底栏兄弟节点）。两个 gorhom sheet 留在 `AppScreen` 内不动（Portal，原地不产生节点）。
- `testID="device-detail-send-button"` 留在 Pressable 上不动。**零新增文案，不需要 `pnpm i18n:extract`。**

## Phase B — 两个发送页（S1）

```diff
--- mobile/src/components/transfer/prepare-progress-bar.tsx:32
-    <View className="flex-1 gap-2">
+    <View className="gap-2">
```
```diff
--- mobile/src/app/send/share-target.tsx:249
-            className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
+            className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
```

两处 `flex-1` 都在**纵向**容器里（`share-target.tsx:235` / `select-device.tsx:260` 的 `<View className="flex-1 gap-2">`，那一层的 `flex-1` 是横向的、**要保留**）。列容器默认 `align-items: stretch`，横向撑满不依赖 `flex-1`。`select-device.tsx` 只吃 `prepare-progress-bar` 那一行修复，自身无需改。

## Phase C — 底距公式（S2，一改 8 处受益）

新增 `mobile/src/hooks/useBottomSafePadding.ts`：

```ts
import { useSafeAreaInsets } from "react-native-safe-area-context";

/**
 * 底部固定区的 paddingBottom = 系统占用（手势条 / home indicator / 三键导航）+ 视觉呼吸位。
 *
 * 两者**相加**，不是 `Math.max` —— 取大在所有现代设备上恒等于 inset
 * （Android 手势条 24dp / 三键 48dp、iOS home indicator 34dp 都 ≥ 呼吸位），
 * 于是主按钮与系统条之间永远零间距。仓内已有的正确写法本来就是相加：
 * `onboarding-scaffold.tsx`（inset + 16）、三个配对页（inset + 24）。
 */
export function useBottomSafePadding(breathing = 12) {
  return useSafeAreaInsets().bottom + breathing;
}
```

```diff
--- mobile/src/components/mobile/screen.tsx:381-385
-  const insets = useSafeAreaInsets();
+  const paddingBottom = useBottomSafePadding();   // 与 pt-3 对称
   …
-      style={{ paddingBottom: Math.max(insets.bottom, 12) }}
+      style={{ paddingBottom }}

--- mobile/src/components/transfer-offer-host.tsx:558-563
-      style={{ paddingBottom: safeArea ? Math.max(insets.bottom, 20) : 20 }}
+      style={{ paddingBottom: safeArea ? paddingBottomSafe : 12 }}
```

⚠️ 一个副作用要盯：`device/groups.tsx:200` 的 `BottomActionBar` 包在 `KeyboardStickyView` 里，键盘弹起时贴键盘，此时 `insets.bottom` 那部分是多余的（改前也多余，改后多 12dp）。这是既有形态，本次不动，但真机要看一眼。

## Phase D — 让同样的误用不可能再发生（S2）

**删掉 `BottomActionArea`（`screen.tsx:356-368`），把它内联进 `HomeDock`。** 它只有 1 个合法调用点（`(main)/index.tsx:485`，`footer` 槽 + 下压不透明 NativeTabs），却已经造成一次功能不可达。加 JSDoc 挡不住——`AppScreen.footer` 上那 8 行警告（`screen.tsx:79-86`）就在旁边，照样没挡住。

```diff
--- mobile/src/components/mobile/screen.tsx
-export function BottomActionArea({ children, className }) { … }   // 整段删除
```
```diff
--- mobile/src/app/(main)/index.tsx:485 / :525
-    <BottomActionArea>
+    // tab 屏专用 dock：走 AppScreen 的 footer 槽，下方压着不透明的 NativeTabs 栏
+    // （它提供背景与底部安全区，见 (main)/_layout.tsx）。stack 屏一律用 BottomActionBar。
+    <View className="gap-2 border-t border-border bg-background px-5 py-4">
-    </BottomActionArea>
+    </View>
```

删完之后，`screen.tsx` 里只剩一个底栏组件，选错的可能性归零。

## Phase E — 补契约到 `DESIGN.md`（S2）

比照已有的 `Node Status Contract` / `Device Card Contract` 体例，新增 `### Bottom Action Contract (mobile)`，写死四条判据：

1. Stack / 详情屏的固定底栏**一律 `BottomActionBar`**（`mobile/src/components/mobile/screen.tsx:374`），放在 `AppScreen` 的 **children** 里、不进 `footer` 槽。它保证三件事：`border-t` + `bg-background`（**不透明，内容不得穿透**）+ `paddingBottom = insets.bottom + 12`。
2. **底距是相加不是取大**：`insets.bottom` 是系统占用，呼吸位是视觉留白。`Math.max` 在所有现代设备上等于零呼吸位。
3. **带固定底栏的页面，内容区必须是滚动容器**（`AppScreen scroll` / 页内 `ScrollView` / `FlatList` / `FlashList`）。底栏是流内兄弟节点、**从不 absolute**，所以内容侧不需要补等高 `paddingBottom`，只留一段呼吸位（`pb-6`~`pb-8`）。
4. 底栏内单个主动作按钮要带 `flex-1`（容器是 `flex-row`）；`flex-1` **不得**出现在底栏内部的纵向容器里（那是 `flexBasis:0%` 作用于高度，会塌）。

## Phase F — sheet 的安全区（含 S1-b，建议单独 PR）

`AppBottomSheet`（`ui/app-bottom-sheet.tsx:175-213`）完全不处理底部安全区，于是 6 个 sheet 各写各的：`pb-8`(32) / `pb-6`(24)×3 / `pb-4`(16) / footer 20。全都不吃 inset。加上 `BottomActionBar` 的 12、`transfer-offer-host` 的 20，**同一个 App 里同一件事有 5 个魔数**。

正确的修法不是抽 `SheetFooter` 组件，是**在 `AppBottomSheet` 里把 inset 吃掉一次**（合进 `contentContainerStyle` 的 `paddingBottom`），六个调用点统一成一个呼吸位。`device/[peerId].tsx:231` 那个显式的 `bottomInset={0}` 与 `:460` 的 `paddingBottom: 142` 魔数（手算 footer 高度，改 footer 内容就漂移、没人会拦）一并处理。

改动面大、与本次 bug 无因果，建议不混进来。

> **2026-08-10 更正：这一 Phase 里混着一条 S1，不能整体当 S2 排期。** 策略 footer 的
> `pb-4` + 全链路无 inset 就是 S1-b 的成因——三键导航下「取消配对」只剩 12dp 可点，
> 而 S0 修好之后用户仍然点不准那颗按钮。它因此**已随 Phase A 就地修掉**
> （footer 自己吃 `useBottomSafePadding()`，见 S1-b）。
>
> 其余五处（纯魔数、纯呼吸位）留在本 Phase，可以慢慢来。但要记住：**S1-b 是就地修的，
> 共性没修**——`AppBottomSheet` 仍然不吃 inset，下一个新建的 sheet 会重新踩一遍。
> 本节原文点名的两处也要更正：`paddingBottom: 142` 魔数**已消失**（改为量测 footer 实高，
> `policyFooterHeight + BOTTOM_BREATHING`）；而那个 `bottomInset={0}` **不是要「一并处理」
> 的对象，是要保留的**——理由见 S1-b。别照原文去把它换成 `insets.bottom`。

---

# 四、严重度分级

## S0 · 功能缺陷：内容永久够不到（`device/[peerId]`）

**不是「间距不好看」。有一颗可交互控件用户完全够不到，而它是一整块功能的唯一入口。**

- 够不到的是 `device-policy-entry`（`[peerId].tsx:410-420`，「策略设置」）。
- 它是**策略 sheet 的唯一入口**（`openPolicySheet` 全文件只此一处调用点，`:465`），而 sheet 的 footer（`PolicyActionFooter`，定义在 `[peerId].tsx:892`、挂载在 `:273`）承载着 **信任级别 / 接收方式 / 阻止设备 / 取消配对**。
- `rg removePairedDevice src/` 确认：**「取消配对」在整个移动端只有这一个 UI 入口**。所以这颗按钮够不到 = 移动端无法解除配对、无法阻止设备、无法改接收策略。

触发条件：展开「链路详情」（`connection-details.tsx:38` 默认 `expanded=false`，折叠时刚好放得下——这是它躲过 review 的原因）。字体放大、更矮的屏、更长的 relay 地址同样触发。

## S1 · 交互缺陷：可用但明显坏

### S1-a · 两个发送页 prepare 期间

进度条压在分割线上方、主按钮被屏幕底缘切断、触控区跑出屏幕。**不升 S0** 是因为那一刻按钮本就 `disabled`（`sending` 态），没有阻塞任何流程；非 prepare 态底栏正常。

### S1-b · 策略 sheet footer 在三键导航下被吃掉 32dp（2026-08-10 从 S2 上调）

**更正：原判「可点，属误触风险」只对手势条成立，对三键导航不成立。** 原文只算了 24dp 那一档就下了结论，于是这条被归进「打磨」——不改的话下一轮会继续把它当作已评估过的低风险跳过。

**审计时的形态**：`PolicyActionFooter` 的底距是硬编码 `pb-4`(16dp)，而 sheet 挂在根 Stack 之外的 `BottomSheetModalProvider` 里、没有任何安全区 padding，宿主 `renderPolicyFooter` 又写了 `bottomInset={0}`——**全链路没有一处补系统占用**。footer 最后一行是一对 `min-h-11`(44dp) 的「阻止设备 / 取消配对」：

| Android 导航模式 | inset | 被系统条覆盖 | 44dp 按钮剩余可点高度 | 判定 |
|---|---|---|---|---|
| 手势条 | 24dp | 8dp | 36dp | 可点（手势条只吃滑动），误触风险 |
| **三键** | **48dp** | **32dp** | **12dp** | **远低于 Material 3 要求的 48dp 最小触控目标** |
| 全屏隐藏 | 0dp | 0 | 44dp | 正常 |

三键那一行不是「不好看」：被覆盖的 32dp 落在系统的**返回 / 主页 / 最近任务**上——它们吃点击（手势条不吃），所以用户瞄着「取消配对」按下去，得到的是被弹出 App。而 `removePairedDevice` 在整个移动端**只有这一个 UI 入口**（见 S0），于是这条与 S0 是同一块功能的两种坏法：S0 是够不到，这条是够到了也点不准。

**不升 S0 的唯一理由**：那 12dp 确实能点中，不是永久不可达。

**已修（同轮）**：`PolicyActionFooter` 的 `pb-4` 换成 `useBottomSafePadding()`（与 `BottomActionBar` 同一个「相加」公式），上面那段推导已作为注释钉在实现旁。`bottomInset={0}` **刻意保留**——安全区只能有一处吃，让给 footer 自己的 padding，`bg-card` 才能一直铺到屏底；用 `bottomInset` 会把整条 footer 上移、下方露缝，而 footer 是绝对定位、滚动内容不为它让位，内容就从缝里穿出来。已知取舍：键盘弹起时那段系统占用成了多余空隙——空一点可以忍，按钮点不到不行。

**剩余的收口**：sheet 的安全区仍未在 `AppBottomSheet` 里统一吃一次（见「6 个 sheet 无安全区」那节）。这一处是就地修好的，不是共性修好了。

## S2 · 打磨

- 7 个 `BottomActionBar` 页主按钮与系统条零间距（用户直接报的那条）；
- 设备详情屏底 96px 死带（两个 `ConfirmDialog` 的 gap，纯视觉）；
- ~~策略 sheet footer 的 `pb-4`~~ → **已上调至 S1-b**（原判只算了手势条 24dp 一档，三键 48dp 下剩 12dp 可点，不属打磨）；
- 6 个 sheet 无安全区、5 个魔数。

---

# 五、验证清单

**仓内 mobile 的自动化覆盖为零** —— 只有 `e2e/desktop`（WebdriverIO + tauri-plugin-wdio），没有 `.maestro/`，`rg "device-policy-entry|send-action-bar"` 在测试文件中零命中。所以下面全是手测，且**建议顺手补一条 Maestro flow**（testID 都是现成的：`connection-details-toggle` / `device-policy-entry` / `device-detail-send-button` / `send-action-bar` / `share-target-action-bar`）：展开链路详情 → scroll → 断言 `device-policy-entry` 可见可点。功能不可达这类回归，只有这种形式测得出来。

| 页面 | 关键动作 | Android | iOS |
|---|---|---|---|
| `device/[peerId]` | **展开「链路详情」**后滚到「策略设置」并点开；sheet 内点到「取消配对」——**这一条必须在三键导航下做**（S1-b 只在 48dp 那档显现，手势条下看着没事） | ✅ 必测 | ✅ 必测 |
| `send/select-device` | 选一个 ≥1GB 的文件触发 prepare，看进度条与按钮 | ✅ 必测 | ✅ 必测 |
| `send/share-target` | 从系统分享进入 + 同上 | ✅ 必测 | ✅ 必测 |
| `send/shared-files` / `inbox/[itemId]` / `transfer/[sessionId]` | 只看底距（Phase C 波及） | ✅ | ✅ |
| `device/groups` | **键盘弹起 / 收起两态**（`KeyboardStickyView`，Phase C 唯一有风险的点） | ✅ 必测 | ✅ 必测 |
| `(main)/index`（HomeDock） | Phase D 内联后不回归（tab 栏之上、`py-4` 不变） | ✅ | ✅ 必测（iOS 26 浮动胶囊） |

Android 三种导航模式各看一次：**手势条 24dp / 三键 48dp / 全屏隐藏 0dp**——`Math.max` → 相加的差别只在前两种显现。**三键那一档不能省**：它是唯一能暴露 S1-b 的档位（系统条吃点击，不像手势条只吃滑动），而本报告第一版恰恰是因为只算了手势条才把 S1-b 误判成 S2。iOS 分 **有 home indicator（iPhone X+，34dp）** 与 **无（iPad / SE）** 两种。

机器门禁（`mobile/` 下）：`pnpm typecheck`。无新增文案，不需要 `pnpm i18n:extract`。

---

# 六、工作量与改动面

| Phase | 文件数 | 净行数 | 说明 |
|---|---|---|---|
| A 设备详情 | 1 | ~±30 | `device/[peerId].tsx` |
| B 发送页 | 2 | 2 行 | `prepare-progress-bar.tsx` + `share-target.tsx`，各删一个 `flex-1` |
| C 底距公式 | 3 | ~+15 | 新 hook + `screen.tsx` + `transfer-offer-host.tsx`，**波及 8 个页面** |
| D 删 `BottomActionArea` | 2 | ~-10 | `screen.tsx` + `(main)/index.tsx` |
| E `DESIGN.md` 契约 | 1 | ~+25 | 新增一节 |
| **A–E 合计** | **9** | **~90 行** | 一个 PR 装得下 |
| F sheet 安全区（另开） | 8 | ~+60 | 与本 bug 无因果，建议单独 PR |

建议提交切分：**A+B 一个 commit（fix）**，**C+D+E 一个 commit（refactor + docs）**——前者可单独 cherry-pick 发补丁版，后者是防复发。

绝对路径清单：
- `/Volumes/yexiyue/SwarmDrop/mobile/src/app/device/[peerId].tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/components/mobile/screen.tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/components/transfer/prepare-progress-bar.tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/app/send/share-target.tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/components/transfer-offer-host.tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/app/(main)/index.tsx`
- `/Volumes/yexiyue/SwarmDrop/mobile/src/hooks/useBottomSafePadding.ts`（新建）
- `/Volumes/yexiyue/SwarmDrop/DESIGN.md`
- 参照实现：`/Volumes/yexiyue/SwarmDrop/mobile/src/app/inbox/[itemId].tsx:497-500`、`/Volumes/yexiyue/SwarmDrop/mobile/src/app/settings/bootstrap-nodes.tsx:206-207`