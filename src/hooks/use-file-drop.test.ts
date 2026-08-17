/**
 * 拖放的回归测试。
 *
 * 这个 hook 的前身用 HTML5 的 `onDragOver` / `onDrop` + `File.path`，在 Tauri v2 下
 * 两条都是死的（webview 截走 OS 拖放，且 v2 移除了 `File.path`），表现为「拖进去
 * 完全没反应」。
 *
 * 第二版按坐标做元素级命中测试，在 Retina Mac 上**同样是「拖了没反应」**——事件里的
 * `PhysicalPosition` 在 macOS/Linux 上其实是逻辑像素（见 hook 里的表）。所以现在
 * 不依赖坐标，这里钉的是剩下那些仍会静默失效的不变量。
 */

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { getCurrentWebview, onDragDropEvent, unlisten } = vi.hoisted(() => ({
  onDragDropEvent: vi.fn(),
  unlisten: vi.fn(),
  getCurrentWebview: vi.fn(),
}));

vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview }));

beforeEach(() => {
  // afterEach 的 resetAllMocks 会连实现一起清掉，这里重新装回默认的可用宿主。
  getCurrentWebview.mockReturnValue({ onDragDropEvent });
});

afterEach(() => {
  // resetAllMocks 而非 clearAllMocks：后者只清调用记录、**不清实现**，于是设了
  // 长期 mockImplementation 的用例（永不 resolve / 恒 reject）会污染后面新增的用例，
  // 让它们在一个从未订阅成功的 hook 上绿着。
  vi.resetAllMocks();
  vi.restoreAllMocks();
});

import { useFileDrop } from "./use-file-drop";

/** 挂载 hook 并返回事件注入器。`view.rerender({ disabled })` 可改运行中的 props。 */
async function mount(options: { disabled?: boolean } = {}) {
  const onDrop = vi.fn();
  let emit!: (payload: unknown) => void;

  onDragDropEvent.mockImplementation((handler: (e: unknown) => void) => {
    emit = (payload) => handler({ payload });
    return Promise.resolve(unlisten);
  });

  const view = renderHook(({ disabled }) => useFileDrop({ onDrop, disabled }), {
    initialProps: { disabled: options.disabled ?? false },
  });
  // 订阅是异步的，等 then 落地
  await act(async () => {});

  return { view, onDrop, emit: (payload: unknown) => act(() => emit(payload)) };
}

describe("useFileDrop", () => {
  it("投放文件时上报路径并熄灭高亮", async () => {
    const { view, onDrop, emit } = await mount();

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(view.result.current).toBe(true);

    emit({ type: "drop", paths: ["/a.txt", "/b.txt"] });
    expect(onDrop).toHaveBeenCalledWith(["/a.txt", "/b.txt"]);
    expect(view.result.current).toBe(false);
  });

  it("非文件拖拽（paths 为空）既不点亮也不上报", async () => {
    // macOS 上拖一段选中的文字或一个链接过来照样触发 enter，只是 paths 为空——
    // wry 的 dragging_entered 不按类型过滤。不挡的话投放区会亮起「可以放」，
    // 松手却什么也不发生。
    const { view, onDrop, emit } = await mount();

    emit({ type: "enter", paths: [] });
    expect(view.result.current).toBe(false);

    emit({ type: "drop", paths: [] });
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("有模态打开时拒收，关闭后恢复", async () => {
    // 整窗口接收没有 z-order 保护：不挡的话，拖到盖在发送页上的对话框（如全局的
    // 传输 offer 弹窗）上松手，文件会被塞进它背后的发送列表。
    const { view, onDrop, emit } = await mount();
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("data-state", "open");
    document.body.append(dialog);

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(view.result.current).toBe(false);

    emit({ type: "drop", paths: ["/a.txt"] });
    expect(onDrop).not.toHaveBeenCalled();

    dialog.remove();
    emit({ type: "drop", paths: ["/a.txt"] });
    expect(onDrop).toHaveBeenCalledWith(["/a.txt"]);
  });

  it("disabled 时忽略投放且不点亮", async () => {
    // 窗口级事件不受 `pointer-events: none` 约束，只能在处理器里挡。
    const { view, onDrop, emit } = await mount({ disabled: true });

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(view.result.current).toBe(false);

    emit({ type: "drop", paths: ["/a.txt"] });
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("拖拽途中 disabled 翻转，高亮两个方向都立刻跟上", async () => {
    // 指针静止时平台可能一条 drag-over 都不再发，所以高亮不能只在事件处理器里算：
    // 转真时会亮在已渲染成禁用态的区域上；转假时（一次发送刚结束）区域显示成不可
    // 投放、松手却真的会收下，观感与行为对不上。
    const { view, emit } = await mount();

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(view.result.current).toBe(true);

    act(() => view.rerender({ disabled: true }));
    expect(view.result.current).toBe(false);

    // 不发任何新事件，仅恢复 disabled
    act(() => view.rerender({ disabled: false }));
    expect(view.result.current).toBe(true);
  });

  it("over 事件不读 paths（它的 payload 里没有这个字段）", async () => {
    // `over` 只带 position。若有人把 `paths.length` 的判定挪到早退之前，拖拽期间
    // 每次指针移动都会在 Tauri 的 listener 回调里抛 TypeError——那里没人接，
    // 控制台一声不响，投放区直接死掉。
    const { view, onDrop, emit } = await mount();

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(() => emit({ type: "over", position: { x: 1, y: 1 } })).not.toThrow();

    expect(view.result.current).toBe(true); // 高亮不受 over 影响
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("leave 熄灭高亮", async () => {
    const { view, emit } = await mount();

    emit({ type: "enter", paths: ["/a.txt"] });
    expect(view.result.current).toBe(true);

    emit({ type: "leave" });
    expect(view.result.current).toBe(false);
  });

  it("props 变化不重新订阅", async () => {
    // hook 靠 latest-ref 读最新回调，订阅只建一次。若有人「简化」成
    // `useEffect(..., [disabled, onDrop])`，而 onDrop 在 FileDropZone 里是每次渲染
    // 新建的内联箭头，就会每渲染一次退订重订一次——重叠窗口里一次投放被处理两次，
    // 同一批文件进两遍发送列表。
    const { view } = await mount();

    act(() => view.rerender({ disabled: true }));
    act(() => view.rerender({ disabled: false }));

    expect(onDragDropEvent).toHaveBeenCalledTimes(1);
  });

  it("卸载时退订", async () => {
    const { view } = await mount();

    view.unmount();

    expect(unlisten).toHaveBeenCalled();
  });

  it("订阅落地前就卸载也会退订", async () => {
    // 不补这一刀，listener 永久留下：每进出一次发送页泄漏一条。
    let resolveListen!: (fn: () => void) => void;
    onDragDropEvent.mockImplementation(
      () => new Promise<() => void>((resolve) => (resolveListen = resolve)),
    );

    const view = renderHook(() => useFileDrop({ onDrop: vi.fn() }));
    view.unmount();

    await act(async () => resolveListen(unlisten));

    expect(unlisten).toHaveBeenCalled();
  });

  it("宿主缺 IPC 时降级而不是掀翻组件树（同步抛的那条路径）", async () => {
    // 真实的非 Tauri 宿主（`pnpm dev` 裸浏览器、未 mock 的 vitest 渲染）里，
    // `getCurrentWebview()` 读 `window.__TAURI_INTERNALS__.metadata` **同步抛**
    // TypeError——抛在 promise 链建立之前，只挂 `.catch()` 抓不到，effect 里的异常
    // 会直接掀掉整棵 React 树。这条钉的就是那个 try/catch。
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    getCurrentWebview.mockImplementation(() => {
      throw new TypeError("Cannot read properties of undefined");
    });

    const view = renderHook(() => useFileDrop({ onDrop: vi.fn() }));
    await act(async () => {});

    expect(view.result.current).toBe(false);
    // 降级必须留痕：静默 catch 会让「它坏了」和「它没被触发」不可区分。
    expect(warn).toHaveBeenCalled();
  });

  it("订阅 promise 被拒时同样降级并留痕", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    onDragDropEvent.mockImplementation(() =>
      Promise.reject(new Error("listen denied")),
    );

    const view = renderHook(() => useFileDrop({ onDrop: vi.fn() }));
    await act(async () => {});

    expect(view.result.current).toBe(false);
    expect(warn).toHaveBeenCalled();
  });
});
