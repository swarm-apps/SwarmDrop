"use client";

// 环境层的加载壳。本体在 [`./ambient-canvas`]，那里也写着这一层为什么存在。
//
// ## 为什么要多这一个文件
//
// 两件事，一个都省不掉：
//
// 1. **`ssr: false` 只能写在 Client Component 里。** 挂载点 `layout.tsx` 是 server
//    component，在那里写 `dynamic(..., { ssr: false })` 会直接构建报错。
// 2. **`dynamic()` 要 import 的必须是另一个模块**，代码分割才发生。写在同一个文件里，
//    ogl 与两段着色器照样进首屏 chunk，那就白分了。
//
// ## 为什么一定要分割出去
//
// 这一层是**纯装饰**，而这个应用启动时要抢的是另一件事：拉 `_bg.wasm` 起节点。
// 用户打开标签页想的是「收个文件」，不是「看背景」——ogl 那 ~30KB 加上着色器编译
// 排在 wasm 前面是纯粹的倒置。`ssr:false` 让它连预渲染 HTML 都不进，挂载后才去取。
//
// 副作用（可接受）：背景比内容晚一两帧出现。这不需要补偿动画——它本来就是一层
// 极缓慢的光，淡入淡出反而会引人去看它。
//
// **挂载点只能是 layout。** 它持有 WebGL context + RAF 循环，是运行时单例，与
// `WebNodeBootstrap` 同类；下放到任何 page 就会变成每路由各起一份 GL context
// （浏览器对同时存活的 context 数量有硬上限，超了会静默丢掉最老的那个）。

import dynamic from "next/dynamic";

const AmbientCanvas = dynamic(() => import("./ambient-canvas"), { ssr: false });

export function AppAmbientBackground() {
  return <AmbientCanvas />;
}
