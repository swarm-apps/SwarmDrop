## 1. 迁移：FTS5 索引与同步触发器

- [ ] 1.1 在 `crates/migration` 新增迁移文件，创建 FTS5 虚拟表 `inbox_fts`（`tokenize = 'trigram'`，external content 指向 `inbox_items`，含预留空列 `extracted_text`）
- [ ] 1.2 创建 `inbox_items` 的 AFTER INSERT / UPDATE / DELETE 触发器，维护 `inbox_fts` 对应行（标题、来源名）
- [ ] 1.3 创建 `inbox_item_files` 的 AFTER INSERT / UPDATE / DELETE 触发器，按 `inbox_item_id` 重新聚合该条目所有文件名 + 相对路径写回 `inbox_fts`
- [ ] 1.4 迁移末尾执行一次全量 rebuild，回填存量 inbox 数据，确保幂等
- [ ] 1.5 在 `migration` 注册该迁移到 Migrator 列表

## 2. core：search_inbox 查询实现

- [ ] 2.1 在 `crates/core` 定义 `InboxSearchHit` 结构（条目 id、标题、来源名、接收时间、文件数、根路径、匹配片段/字段）
- [ ] 2.2 实现 `search_inbox(query: &str, limit: usize) -> Result<Vec<InboxSearchHit>>`：FTS `MATCH` 查询 + `snippet()` + 按 rank 排序 + `limit` 截断
- [ ] 2.3 join `inbox_items` 应用过滤：排除 `deleted_at` 非空；默认排除 `archived_at` 非空（保留可选包含归档的形参）
- [ ] 2.4 处理空查询/无命中：返回空列表不报错

## 3. core：单元测试

- [ ] 3.1 同步测试：新增条目后立即可搜；更新/删除文件后索引同步（覆盖三类触发器）
- [ ] 3.2 过滤测试：`deleted_at` 条目不返回；默认排除 `archived_at`，显式包含时返回
- [ ] 3.3 排序与 limit 测试：多命中按相关度降序、截断到 limit
- [ ] 3.4 CJK 测试：中文关键词（如"合同"）能命中标题/文件名
- [ ] 3.5 回填测试：对预置存量数据跑迁移后历史条目可搜
- [ ] 3.6 `extracted_text` 为空时检索行为不受影响

## 4. 桌面端：Tauri command

- [ ] 4.1 实现 `search_inbox` Tauri command，转发到 core `search_inbox`，复用托管数据库连接
- [ ] 4.2 数据库未就绪时返回明确错误而非 panic
- [ ] 4.3 注册到 `invoke_handler`
- [ ] 4.4（可选）`src/commands/` 加 TypeScript 封装，供后续 UI 搜索框使用（本期不做 UI）

## 5. MCP：search_inbox 与 get_inbox_file 工具

- [ ] 5.1 在 `src-tauri/src/mcp/tools.rs` 新增 `search_inbox` Tool（参数 `query` + 可选 `limit`，默认 20），经 `AppHandle` 调用桌面检索能力，输出结构化命中列表
- [ ] 5.2 新增 `get_inbox_file` Tool（参数：条目 id + 文件标识 id/相对路径），返回 `local_path`、文件名、大小、`missing`
- [ ] 5.3 `missing`/路径不可达时返回缺失标识，不返回可误用路径；条目/文件不存在返回 `isError: true`
- [ ] 5.4 数据库未就绪时返回 `isError: true`，服务不崩溃
- [ ] 5.5 扩展 `swarmdrop://guide`（`mcp-guide.md`）：补充"先 search_inbox 再 get_inbox_file"的顺序与"仅本机 inbox"声明

## 6. 验证

- [ ] 6.1 `cargo build` 与 core 测试全过
- [ ] 6.2 用 MCP 客户端（curl 或 Claude Desktop）实测 `tools/list` 含两个新工具
- [ ] 6.3 实测 `search_inbox` 命中预置条目、`get_inbox_file` 返回正确路径、缺失文件正确报缺失
- [ ] 6.4 确认移动端拉取共享 core 后迁移静默生效、现有功能不回归（仅编译/启动验证）
