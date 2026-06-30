## 1. 迁移：standalone FTS5 表与存量回填

- [x] 1.1 在 `crates/migration` 新增迁移文件，创建 standalone FTS5 虚拟表 `inbox_fts`（`tokenize = 'trigram'`；列 `item_id UNINDEXED`、`title`、`source_name`、`files_text`、`extracted_text`；`extracted_text` 预留留空）
- [x] 1.2 迁移末尾显式回填存量：`INSERT INTO inbox_fts(item_id,title,source_name,files_text,extracted_text) SELECT i.id, i.title, i.source_name, <聚合文件名+相对路径>, '' FROM inbox_items i JOIN inbox_item_files f ... GROUP BY i.id`（幂等：回填前确保 `inbox_fts` 为空 / 可重入）
- [x] 1.3 在 `migration` 注册该迁移到 Migrator 列表；`down` 仅 `DROP inbox_fts`（业务表不受影响）

## 2. core：inline 写入 + search_inbox 查询

- [x] 2.1 在 `ensure_inbox_item_for_completed_receive_session` 的事务内、文件循环之后，inline 写一条聚合后的 FTS 行（与 1.2 复用同一段聚合逻辑：`files_text` = 文件名 + 相对路径空格拼接，`extracted_text` 留空）
- [x] 2.2 定义 `InboxSearchHit` 结构（条目 id、标题、来源名、接收时间、文件数、根路径、匹配片段；派生 `serde::Serialize` + `#[cfg_attr(feature = "specta", derive(specta::Type))]`，对齐 `InboxItemSummary` 现有写法）
- [x] 2.3 实现 `search_inbox(query: &str, limit: usize, include_archived: bool) -> Result<Vec<InboxSearchHit>>`：归一化查询（去首尾空白）→ 对索引文本列做 `LIKE '%kw%'` 子串匹配（≥3 字走 trigram 索引、<3 字小表全扫）→ `received_at` 倒序 → `limit` 截断
- [x] 2.4 join `inbox_items` 应用过滤：排除 `deleted_at` 非空；默认排除 `archived_at` 非空（`include_archived` 显式包含时返回）
- [x] 2.5 匹配片段在 Rust 端按子串命中位置切窗口生成（统一格式，不用 FTS `snippet()`）
- [x] 2.6 处理空查询 / 无命中：返回空列表不报错

## 3. core：单元测试

- [x] 3.1 inline 写入测试：新增 item（含多文件）后立即可按标题 / 文件名搜到
- [x] 3.2 过滤测试：`deleted_at` 条目不返回；默认排除 `archived_at`，`include_archived` 时返回
- [x] 3.3 排序与 limit 测试：多命中按 `received_at` 倒序、截断到 limit
- [x] 3.4 CJK 两字测试：`search_inbox("合同", ...)` 命中标题 / 文件名含"合同"的条目（**断言不返回空**，守住 trigram 2 字边界这个回归点）
- [x] 3.5 回填测试：对预置存量数据跑迁移后历史条目可搜
- [x] 3.6 `extracted_text` 为空时检索行为不受影响（仅基于标题、来源名、文件名、相对路径）

## 4. 桌面端：Tauri command

- [x] 4.1 实现 `search_inbox` Tauri command（`query` + 可选 `limit` / `include_archived`），转发到 core `search_inbox`，复用托管 `DatabaseConnection`
- [x] 4.2 数据库未就绪时返回明确错误而非 panic（由 Tauri `State<DatabaseConnection>` 注入机制保证，缺失时返回错误不 panic）
- [x] 4.3 注册到 `invoke_handler`
- [ ] 4.4（可选）`src/commands/` 加 TypeScript 封装，供后续 UI 搜索框使用（本期不做 UI）

## 5. MCP：search_inbox 与 get_inbox_file 工具

- [x] 5.1 在 `src-tauri/src/mcp/tools.rs` 新增 `search_inbox` Tool（参数 `query` + 可选 `limit`，默认 20），经 `AppHandle` 取 `DatabaseConnection` 调 core `search_inbox`，输出结构化命中列表（每个命中含文件列表的**文件名 + 相对路径**，供下钻）
- [x] 5.2 新增 `get_inbox_file` Tool（参数：条目 id + 文件标识 file id / 相对路径），**复用 core `get_inbox_item_detail`** 取文件，返回 `local_path`、文件名、大小、`missing`（不新增 core 仓储）
- [x] 5.3 `missing` / 路径不可达时返回缺失标识，不返回可误用路径；条目 / 文件不存在返回 `isError: true`
- [x] 5.4 数据库未就绪时返回 `isError: true`，服务不崩溃
- [x] 5.5 扩展 `swarmdrop://guide`（`src-tauri/docs/mcp-guide.md`）：补充"先 `search_inbox` 再 `get_inbox_file`"的顺序与"仅本机 inbox"声明

## 6. 验证

- [x] 6.1 `cargo build` 与 core 测试全过（core 87 + e2e 13 测试通过；migration `inbox_fts` 测试通过；桌面壳 `cargo check` + 三 crate `clippy` 零 warning）
- [x] 6.2 用 MCP 客户端实测 `tools/list` 含两个新工具 ——✅ MCP Inspector CLI 连 `http://127.0.0.1:19527`，`tools/list` 返回 5 工具含 `search_inbox`/`get_inbox_file`（inputSchema 正确）
- [x] 6.3 实测 `search_inbox` 命中、`get_inbox_file` 路径/缺失 ——✅ 真实 app + bundled FTS5 + 真实数据：MCP `search_inbox("editor")` 命中；command 层 2 字 `md` 靠 LIKE 兜底命中、`large`/`bin` trigram 命中、无命中/空查询返回空；`get_inbox_file` 正常返回 `localPath`(35993B)、不存在条目 `isError:true`；前端搜索框过滤+高亮+无匹配空态+清空恢复全通过
- [ ] 6.4 确认移动端拉取共享 core 后迁移静默生效、现有功能不回归（仅编译 / 启动验证）——【跨仓手测】本仓 core 编译 + `inbox_fts` 迁移测试已验证迁移平台无关、幂等；SwarmDrop-RN 拉取后的静默生效需在该仓验证
