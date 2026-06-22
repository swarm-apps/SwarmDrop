"use client";

// 客户端 Mermaid 渲染组件。Fumadocs 静态导出（output: 'export'）不自带 Mermaid，
// 这里走「'use client' + 动态 import('mermaid')」：SSG 时只产出占位 div，水合后在浏览器
// 渲染 SVG。mermaid 依赖 DOM，不能 SSR。暗色跟随 Fumadocs 切到 <html> 上的 `.dark` class
// （用 MutationObserver 监听），避免直接依赖 next-themes。
import { useEffect, useId, useRef, useState } from "react";

function useIsDark(): boolean {
  const [dark, setDark] = useState(false);
  useEffect(() => {
    const el = document.documentElement;
    const update = () => setDark(el.classList.contains("dark"));
    update();
    const obs = new MutationObserver(update);
    obs.observe(el, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);
  return dark;
}

export function Mermaid({ chart }: { chart: string }) {
  const rawId = useId();
  const id = `mmd-${rawId.replace(/[^a-zA-Z0-9]/g, "")}`;
  const dark = useIsDark();
  const [svg, setSvg] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const { default: mermaid } = await import("mermaid");
      mermaid.initialize({
        startOnLoad: false,
        theme: dark ? "dark" : "default",
        securityLevel: "loose",
        fontFamily: "inherit",
      });
      try {
        const { svg } = await mermaid.render(id, chart, containerRef.current ?? undefined);
        if (!cancelled) setSvg(svg);
      } catch {
        // 渲染失败时静默：保留占位，不阻塞页面
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [chart, dark, id]);

  return (
    <div
      ref={containerRef}
      className="my-4 flex justify-center overflow-x-auto"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: mermaid 自生成可信 SVG
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
