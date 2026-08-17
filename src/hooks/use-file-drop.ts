/**
 * use-file-drop
 * 桌面端文件拖放。
 *
 * **拖放走 Tauri 的窗口级事件，不是 HTML5 的 DOM 事件。** 两条原因各自独立成立：
 *
 * 1. Tauri v2 的 `dragDropEnabled` 默认为 `true`，webview 会把 OS 的拖放截走转成
 *    原生事件，于是元素上的 `onDragOver` / `onDrop` **一次都不触发**——高亮不亮、
 *    回调不跑，看起来就是「拖进去什么也没发生」。
 * 2. 就算把它关掉退回 DOM 事件，v2 也已经移除了 webview `File` 对象上的 `path`
 *    （v1 的非标准扩展），而后端 `FileSource` 只有 `{ type: "path" }` 一个变体——
 *    拿不到真实路径就没法发送。
 *
 * **投放目标是整个窗口，刻意不按坐标做元素级命中测试**——理由见 `UseFileDropOptions`
 * 上方的长注释，那不是偷懒，是唯一能在三个平台上都不出错的做法。
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";

/**
 * ## 为什么不做坐标命中测试
 *
 * 事件带一个 `position`，看起来足够把投放归到具体元素上。**但那个坐标的单位三个平台
 * 不一致，而类型名在撒谎**（2026-08-17 读 wry 0.55.1 / tauri-runtime-wry 2.11.4 源码核实）：
 *
 * | 平台 | wry 取值处 | 实际单位 |
 * |---|---|---|
 * | macOS | `NSDraggingInfo.draggingLocation()` + `NSView.frame()` | AppKit points = **逻辑** |
 * | Linux | GTK widget 坐标 | **逻辑** |
 * | Windows | `ScreenToClient` | **物理** |
 *
 * 而 `tauri-runtime-wry/src/lib.rs:4872` 一律包成 `PhysicalPosition::new(x, y)`，
 * **不做任何缩放**。于是同一个字段在 macOS/Linux 上其实是逻辑像素，只有 Windows 名副其实。
 *
 * 本仓最早那版按字面意思除以 `devicePixelRatio`，在 Retina Mac 上坐标直接减半，命中恒为假
 * ——**和「根本没修」长得一模一样**，而且低分屏开发机上完全正常。
 *
 * 可以按平台分叉，但那要赌对三个平台的推断，且只有 macOS 能实测。**不依赖坐标就没有这个
 * 风险**：hook 只在投放区挂载时订阅（当前只有发送页），窗口级事件的本意本来就是「投给这
 * 个窗口」，同类产品（LocalSend 等）也是整窗口接收。
 *
 * 代价是失去两件事，都不值得用「某平台静默失效」去换：
 * - **z-order**：拖到盖在页面上的对话框 / toast 上松手也会被收下。后果轻微且可见（文件进了
 *   列表，用户能直接移除），不是静默错误。
 * - **多投放区仲裁**：同屏挂两个实例会各收一次。当前只有一个消费者，且发送页那两处是
 *   `hasFiles ? compact : full` 的互斥分支。**真要同屏多区，必须先实测三平台坐标语义**，
 *   别凭 `PhysicalPosition` 这个名字想当然。
 */
interface UseFileDropOptions {
  disabled?: boolean;
  /** 投放时回调，`paths` 可能是文件也可能是文件夹；空投放不会触发。 */
  onDrop: (paths: string[]) => void;
}

/** @returns 是否有文件正拖在窗口上方（用于画高亮）。 */
export function useFileDrop({ disabled, onDrop }: UseFileDropOptions): boolean {
  // 订阅只建一次。listen 是异步的，若跟着 props 变化重订阅，快速改动期间会出现
  // 「旧的还没退、新的已经进」的重叠窗口，同一次投放被处理两次。
  const latest = useRef({ disabled, onDrop });
  // 只在 commit 后写。渲染期直接赋值的话，被 React 丢弃的推测性渲染
  // （transition / 路由预渲染）也会覆盖进来，于是判定用的是用户从未见过的状态。
  useLayoutEffect(() => {
    latest.current = { disabled, onDrop };
  });

  // 「窗口上方正拖着文件」是平台事实，与 disabled 无关，所以单独记一份；对外暴露的
  // `isOver` 是它与 `!disabled` 的合取。
  //
  // 不能只在事件处理器里算：指针静止时平台可能一条 drag-over 都不再发（Windows 的
  // DragOver 由指针移动驱动），于是 disabled 两个方向都会卡住——转真时高亮亮在已经
  // 渲染成 `opacity-50` 的禁用区上，转假时（一次发送刚结束）区域又一直显示成不可投放，
  // 可松手却真的会收下，观感与行为对不上。
  const [fileOverWindow, setFileOverWindow] = useState(false);
  const isOver = fileOverWindow && !disabled;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    // `getCurrentWebview()` 在非 Tauri 宿主下**同步抛** TypeError
    // （`window.__TAURI_INTERNALS__.metadata` 读的是 undefined 的属性），
    // 抛在 promise 链建立之前——只挂 `.catch()` 抓不到它，会直接掀掉整棵 React 树。
    try {
      getCurrentWebview()
        .onDragDropEvent(({ payload }) => {
          const current = latest.current;

          if (payload.type === "leave") {
            setFileOverWindow(false);
            return;
          }
          // `over` 的 payload **只有 position 没有 paths**，读 `paths.length` 会当场抛。
          // 而它也不携带新信息：拖着的东西不会中途变，位置又不看。
          if (payload.type === "over") return;

          // 非文件拖拽（选中的文字、浏览器里的链接）在 macOS 上照样触发 enter，只是
          // `paths` 为空——wry 的 `dragging_entered` 不按类型过滤。不挡的话投放区会亮起
          // 「可以放」，松手却什么也不发生。Windows 反过来：`DragEnter` 里
          // `iterate_filenames` 拿不到 HDROP 就直接 return，**非文件拖拽根本不发事件**
          // （文件拖拽是正常发的）。挡掉空 paths 把两个平台的表现拉齐。
          const hasFiles = payload.paths.length > 0 && !isModalOpen();

          if (payload.type === "drop") {
            setFileOverWindow(false);
            if (hasFiles && !current.disabled) current.onDrop(payload.paths);
            return;
          }
          setFileOverWindow(hasFiles);
        })
        .then((fn) => {
          // 组件可能在订阅落地前就卸载了。不补这一刀，listener 会永久留下——
          // 每进出一次发送页泄漏一条，且它们都还在往已卸载的组件里 setState。
          if (cancelled) fn();
          else unlisten = fn;
        })
        .catch(reportUnavailable);
    } catch (error) {
      reportUnavailable(error);
    }

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return isOver;
}

/** 拖放不可用是增强型降级，不打扰用户；但必须留痕，否则坏没坏都不知道。 */
function reportUnavailable(error: unknown) {
  console.warn("[use-file-drop] 拖放订阅失败，该功能不可用：", error);
}

/**
 * 有模态打开时拒收。
 *
 * 投放目标是整个窗口、没有 z-order 保护（理由见上方），所以要靠这条把模态语义补回来
 * ——否则拖到盖在页面上的对话框上松手，文件会被塞进它背后的列表，手势与结果之间
 * 没有任何视觉关联。选中 Radix 的两种角色：`Dialog` 是 `dialog`，`AlertDialog` 是
 * `alertdialog`，打开时都带 `data-state="open"`。
 */
function isModalOpen(): boolean {
  return (
    document.querySelector(
      '[data-state="open"][role="dialog"], [data-state="open"][role="alertdialog"]',
    ) !== null
  );
}
