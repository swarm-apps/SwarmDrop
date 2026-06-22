# @swarmdrop/docs

SwarmDrop 官网 + 文档站，基于 [Fumadocs](https://fumadocs.dev)（Next.js 静态导出）。

## 开发

```bash
pnpm install
pnpm dev        # http://localhost:3000
```

## 构建

```bash
pnpm build      # 静态导出到 out/
pnpm start      # 本地预览 out/
```

## 目录

```
docs/
├── app/
│   ├── (home)/         # 官网首页
│   ├── docs/           # 文档路由（fumadocs）
│   ├── api/search/     # 静态搜索索引（mandarin 分词）
│   └── global.css      # 品牌色 + 首页动效
├── components/         # provider / search / mermaid / mdx / swarm-visual
├── content/docs/       # 文档内容（.mdx + meta.json）
├── lib/                # source / shared / site / layout.shared
└── next.config.mjs     # output: export + basePath
```

## 部署

推送到 `main` 且改动 `docs/**` 时，`.github/workflows/docs.yml` 自动构建静态站点并发布到
GitHub Pages（`PAGES_BASE_PATH=/SwarmDrop`）。线上地址：<https://swarm-apps.github.io/SwarmDrop>。

## 写文档

在 `content/docs/` 下新增 `.mdx` 文件，并在所在目录的 `meta.json` 的 `pages` 数组里登记顺序。
架构图用 Mermaid 组件（需在文件顶部 `import { Mermaid } from "@/components/mermaid"`）。
