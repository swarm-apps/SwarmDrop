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
└── next.config.mjs     # output: export + basePath（Pages 子路径）
```

## 部署

推送到 **`main` 或 `develop`** 且改动 `docs/**` 时，`.github/workflows/docs.yml` 自动构建静态
站点并发布到 GitHub Pages。目标地址：<https://swarm-apps.github.io/SwarmDrop/>。

站点部署在 **Pages 子路径 `/SwarmDrop`** 下。CI 注入两个 env：`PAGES_SITE_ORIGIN`（供
`metadataBase` / sitemap 出绝对 URL；CI 下缺它会直接构建失败，见 `lib/site.ts`）与
`PAGES_BASE_PATH`。

子路径下有两条约束：

1. **裸字符串路径必须手动拼 `BASE_PATH`** —— `next/link`、`next/image` 会自动加前缀，
   但 `<img src="/x">` 与 metadata 里的 `/x` 不会（`/x` 解析时把 base path 整段替换掉）。
   已知需要拼的只有 `lib/shared.ts` 的 `appIconPath`。
2. **`public/` 下的纯 HTML 完全拿不到前缀** —— 配对落地页 `public/p/index.html` 因此一律用
   **相对路径**（`../app/devices/`），并从 `location.pathname` 推导自己的 base。这样它在
   「子路径」与「将来的域名根」两种部署下都不用改。

### 自定义域名（`swarmapp.cn`）—— 待备案，尚未启用

域名已实名，但**没有备案**。境内注册商可以随时停掉未备案 `.cn` 的解析，而这个域名一旦成为
邀请链接的载体，配对功能就挂在一个可被第三方关停的开关上。所以整件事延后到备案完成。

备案下来后要动的地方（**四处一起改，漏一处就是静默失败**）：

| 位置 | 改什么 |
|---|---|
| Settings → Pages → Custom domain | 填 `swarmapp.cn`（权威源，见下） |
| `docs.yml` | `PAGES_SITE_ORIGIN` 换域名、**删掉** `PAGES_BASE_PATH` |
| `next.config.mjs` | 删掉 `basePath`（连带 `lib/shared.ts` 的 `appIconPath` 拼接） |
| `crates/invite` 的 `INVITE_URL_PREFIX` | 换成 `https://swarmapp.cn/p/#`，**旧前缀留在 `ACCEPTED_URL_PREFIXES` 里** |

最后一条是关键：邀请活 24h，迁移期两种链接会同时在外面飘，受理列表让在途邀请不会一夜作废。
移动端与 Web 端已同时受理两个前缀，**不需要跟着改**。

### `CNAME` 文件在本仓**无效**

本仓是 **workflow 型** Pages 部署（`actions/deploy-pages`），GitHub 文档明确写了这种情况下
「不会创建 `CNAME` 文件，已存在的 `CNAME` 文件会被忽略」。所以域名的权威源是
**仓库 Settings → Pages → Custom domain**（等价于 `PUT /repos/{owner}/{repo}/pages`），
**不是仓库里的文件** —— 早期版本在 `public/CNAME` 放过一个，那是装饰品，已删除。


## 写文档

在 `content/docs/` 下新增 `.mdx` 文件，并在所在目录的 `meta.json` 的 `pages` 数组里登记顺序。
架构图用 Mermaid 组件（需在文件顶部 `import { Mermaid } from "@/components/mermaid"`）。
