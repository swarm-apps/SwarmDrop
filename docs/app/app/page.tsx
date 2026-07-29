"use client";

// `/app` 只是入口，落点是设备页（同桌面端：设备关系是应用首页）。
//
// 静态导出（`output: "export"`）下没有服务端，`next/navigation` 的 `redirect()` 与
// next.config 的 `redirects()` 都用不了——只能在客户端跳。渲染的不是空白 loading 而是一条
// 真链接：JS 尚未加载或被禁用时，用户仍然点得进去，不会卡在一个永远转圈的页面上。

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect } from "react";
import { APP_HOME } from "./_lib/nav";

export default function AppIndexPage() {
  const router = useRouter();

  useEffect(() => {
    router.replace(APP_HOME);
  }, [router]);

  return (
    <p className="text-sm text-fd-muted-foreground">
      正在进入{" "}
      <Link href={APP_HOME} className="font-medium text-fd-foreground underline underline-offset-2">
        设备
      </Link>
      …
    </p>
  );
}
