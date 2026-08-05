//! 收件箱领域模型与规则（DTO + 各存储实现共用的判据）。
//!
//! 收件箱是「已接收内容索引」，与 transfer_sessions / transfer_files 的过程账本分开维护。
//! 本模块只放**数据与规则**；端口 trait [`crate::store::InboxStore`] 留在 `store.rs`
//! —— 那里是端口定义的落点，`TransferStore` 的 supertrait 组合也在那，搬走会让端口
//! 定义处读起来断成两截。
//!
//! 这里的规则由**所有存储实现共用**：SQL 侧（`swarmdrop-storage-sql`）与 Web 侧
//! （`crates/web`）各自从自己的行类型构造 [`InboxFileFacts`] 再调它们。分叉即意味着
//! 同一批文件在两端得到不同的内容指纹、不同的检索命中集合。
//!
//! **本模块不产出面向用户的展示串。** 条目的展示标题由各端按当前 locale 从
//! [`InboxItemSummary::primary_file_name`] + `item_count` 生成；曾经有一个 `inbox_title`
//! 在这里拼中文散文并落库，那是「后端只发结构化参数、翻译发生在呈现边缘」这条原则
//! （见 openspec `localize-backend-strings`）的最后一处缺口，已于 2026-08 删除。
//!
//! **wasm 硬约束**：本模块只吃纯 scalar 类型，绝不出现 `entity::*::ModelEx`
//! —— 它是 `HasMany` / `HasOne` 的宿主，一旦上签名就把 sea-orm 的关系机制拖进 wasm
//! target。`From<ModelEx>` 之类的转换留在各自的存储实现里。

use uuid::Uuid;

use crate::host::FileAccess;
use crate::store::TransferProjection;
use crate::{AppError, AppResult};

/// 收件箱列表条目 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemSummary {
    pub id: Uuid,
    pub transfer_session_id: Option<Uuid>,
    pub source_peer_id: String,
    pub source_name: String,
    pub source_kind: entity::InboxSourceKind,
    pub content_kind: entity::InboxContentKind,
    /// 条目内第一个文件的文件名（「第一个」的定义见 [`InboxFileFacts`] 的顺序契约）；
    /// 零文件条目为 `None`。
    ///
    /// **展示标题由各端从它 + [`item_count`](Self::item_count) 生成**，理由见模块文档。
    /// `Option` 而不是空串：「没有文件」与「文件名恰好是空串」必须在类型上分得开。
    pub primary_file_name: Option<String>,
    pub item_count: i32,
    pub total_size: i64,
    pub root_path: Option<String>,
    pub content_hash: Option<String>,
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub missing: bool,
}

/// 收件箱文件 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemFileEntry {
    pub id: i32,
    pub transfer_file_id: Option<i32>,
    /// 条目根之下的相对路径。**Web 宿主删文件用这个**——OPFS 的键就是它。
    pub relative_path: String,
    pub name: String,
    pub size: i64,
    pub checksum: String,
    /// 宿主可直接操作的完整路径。**桌面 / 移动删文件用这个**（那边是真实文件系统路径）；
    /// **Web 上它是带 `opfs:/` 前缀的展示值，喂给 `remove_path` 会去找一个叫 `opfs:` 的目录**。
    ///
    /// 两个路径字段并存且「该用哪个」按端不同，是这个 DTO 最容易踩空的地方——所以写在这里，
    /// 而不是让每个宿主自己从别处推断。
    pub local_path: String,
    pub missing: bool,
}

/// 收件箱详情 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemDetail {
    #[serde(flatten)]
    pub item: InboxItemSummary,
    pub files: Vec<InboxItemFileEntry>,
    pub transfer: Option<TransferProjection>,
}

/// 收件箱搜索命中（item 粒度）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxSearchHit {
    pub id: Uuid,
    /// 同 [`InboxItemSummary::primary_file_name`]：命中条目的展示标题由调用方按当前
    /// locale 从它 + `item_count` 生成，命中结果里不带预拼接的标题。
    pub primary_file_name: Option<String>,
    pub source_name: String,
    pub item_count: i32,
    pub root_path: Option<String>,
    pub received_at: i64,
    /// 命中所在文本的片段（在 Rust 端按子串位置切窗口生成）。
    ///
    /// `None` = **不该渲染片段行**：命中的是首文件名或来源名（条目行上已经显示着——
    /// 标题的内容就是首文件名），或一个候选都没命中。
    /// 判据在 [`inbox_snippet`]，三端不要各判一遍。
    pub snippet: Option<String>,
    /// 该条目下的文件（文件名 + 相对路径），供 get_inbox_file 下钻。
    pub files: Vec<InboxHitFile>,
}

/// 搜索命中条目下的文件标识（供下钻定位）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxHitFile {
    pub name: String,
    pub relative_path: String,
}

/// 收件箱规则的中立文件视图——两端各自从自己的行类型（SQL 的 `ModelEx` /
/// Web 的 IndexedDB 记录）构造，规则本身不认识任何存储类型。
///
/// **顺序契约**：调用方递进来的切片顺序**就是**该条目的文件顺序，两处依赖它：
/// [`inbox_content_hash`] 按此顺序累加（字节级契约，顺序一变哈希就变），
/// `primary_file_name` 取第 0 个。两端的顺序来源分别是——
/// SQL 侧 `transfer_files.id` 升序（条目由**传输文件行**建成，`crates/storage-sql`
/// 在构造 facts 前显式排一次；`inbox_item_files` 随后按同一顺序插入，故它的自增 id
/// 升序与此一致，迁移回填即依赖这条），Web 侧条目内序号 0..n（写入时定、之后不变）。
///
/// 这条契约一直被依赖，但直到 2026-08 才写下来，且 SQL 侧当时**没有任何排序**
/// ——靠 SQLite「不排序时按 rowid 返回」这一实现行为兜着。加 join、改查询计划或换后端
/// 都会静默改掉哈希，所以那处已补上显式排序。
pub struct InboxFileFacts<'a> {
    pub name: &'a str,
    pub relative_path: &'a str,
    pub checksum: &'a str,
    pub size: i64,
}

/// 条目的首文件名：零文件条目为 `None`。
///
/// **它只负责「取第 0 个」，不负责「第 0 个是谁」**——顺序由调用方保证，见
/// [`InboxFileFacts`] 的顺序契约。之所以仍收成一个具名函数而不是让两端各写
/// `files.first()`：将来若要改「哪个文件算首文件」（比如跳过隐藏文件、取最大的那个），
/// 这里是唯一的落点，而分散的 `.first()` 会漏改一端且不报错。
pub fn inbox_primary_file_name(files: &[InboxFileFacts<'_>]) -> Option<String> {
    files.first().map(|file| file.name.to_string())
}

/// 条目内容指纹：逐文件累加 `relative_path ‖ 0x00 ‖ checksum ‖ size_le` 的 blake3。
///
/// 这是**跨端去重的唯一判据**，累加顺序与分隔字节是字节级契约：
/// 任一端改动一个字节，同一批文件在两端就会算出不同的指纹，这个字段随即作废。
/// `inbox_content_hash_known_vector` 用钉死的十六进制串守着它。
pub fn inbox_content_hash(files: &[InboxFileFacts<'_>]) -> String {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(file.checksum.as_bytes());
        hasher.update(&file.size.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// 检索索引的聚合文本：每个文件取 `"{name} {relative_path}"`，条目内以空格拼接。
///
/// 它就是「检索覆盖面」的定义：SQL 侧把它写进 `inbox_fts.files_text`，
/// Web 侧在内存里对同一段文本做子串扫描——两端扫的必须是同一段文本，
/// 否则同一次搜索的命中集合会分叉。
pub fn inbox_files_text(files: &[InboxFileFacts<'_>]) -> String {
    files
        .iter()
        .map(|file| format!("{} {}", file.name, file.relative_path))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 由接收会话的 `origin` 列派生收件箱 `source_kind`：MCP/代理来源 → `Mcp`，否则 `PairedDevice`。
/// 历史 NULL / 未知值经 [`crate::protocol::TransferOrigin::from_db_string`] 回退 `Human` → `PairedDevice`。
pub fn inbox_source_kind(origin: Option<&str>) -> entity::InboxSourceKind {
    match crate::protocol::TransferOrigin::from_db_string(origin.unwrap_or("human")) {
        crate::protocol::TransferOrigin::Mcp { .. } => entity::InboxSourceKind::Mcp,
        crate::protocol::TransferOrigin::Human => entity::InboxSourceKind::PairedDevice,
    }
}

/// 「这个会话该不该进收件箱」的**唯一判据**：接收方向 + 终态 + 终态原因为完成。
///
/// 它决定收件箱里有什么，而三处存储实现（SQL 的 `ensure_*`、Web 的 `ensure_from_session`、
/// Web 的补建扫描）此前各写了一遍同一个三段合取。漏改一处的表现不是报错，是
/// 「同一批会话在浏览器建了条目、在桌面没建」——最难对齐的那类分叉。
///
/// SQL 的补建扫描仍保留自己的 `WHERE`（数据库里调不到 Rust 函数），但那三个 filter
/// 必须与本函数同义，改一边就得改另一边。
pub fn is_completed_receive(session: &entity::transfer_session::Model) -> bool {
    session.direction == entity::TransferDirection::Receive
        && session.phase == entity::TransferPhase::Terminal
        && session.terminal_reason == Some(entity::TerminalReason::Completed)
}

/// 检索命中判据的**规范定义**：大小写不敏感子串，覆盖 source_name / files_text /
/// extracted_text 三列。
///
/// **为什么没有「标题」列。** 展示标题是 `files` 的纯函数、且是有损投影——单文件条目的
/// 标题就是文件名（已被 `files_text` 覆盖），多文件条目只多出「等 N 个文件」，
/// 零文件条目只多出「空传输」。它不可能带来 `files_text` 没有的信息，只可能带来模板噪音：
/// 所有多文件条目的标题都含「个文件」，搜「文件」会把它们整批捞出来。
/// **判据可复用**：由已索引字段派生的展示串一律不进索引。
///
/// SQL 侧不调用它（那边的匹配在数据库里由 `LIKE ... ESCAPE '\'` 完成），
/// 但 **SQL 的那段 `LIKE` 必须复刻本函数的语义**：同一个查询词在两端要给出同一个
/// 命中集合。把判据写成一个可读可测的函数，是为了让「两端同义」有一个锚点，
/// 而不是散在两处的口头约定——[`INBOX_MATCH_CASES`] 是那个锚点的可执行形式，
/// 两端的单测灌同一批语料、断言同一批 expected。
///
/// **`extracted_text` 为什么是 `Option`。** 它是 SQL 侧 `inbox_fts` 的第四列（文档正文
/// 抽取结果）。浏览器没有文本抽取，Web 侧恒传 `None`——不是「传空串」：空串在语义上是
/// 「抽过、没抽到东西」，`None` 才是「这一端没有这个能力」，而这条差异该在签名上看得见。
/// 此前本函数**根本没有这个入参**，于是它自称规范定义、SQL 却多匹配一列，规范与实现
/// 从一开始就是错位的。
///
/// 空查询（含全空白）恒不命中——与 SQL 侧 `search_inbox` 对空查询直接返回空列表一致。
///
/// **已知差异（不要试图消除）**：Rust 的 `to_lowercase()` 按 Unicode 折叠大小写，
/// SQLite 的 `LIKE` 默认只折叠 ASCII。于是「Ä」查「ä」在 Web 端命中、在桌面不命中。
/// 消除它要么给 SQLite 编 ICU 扩展、要么在 Rust 侧退化成 ASCII 折叠（把 Web 端做差），
/// 两条都比这条差异本身贵：非 ASCII 的大小写变体在文件名与设备名里近乎不出现，
/// 而 CJK 根本没有大小写。[`INBOX_MATCH_CASES`] 因此只放 ASCII 的大小写用例。
pub fn inbox_matches(
    query: &str,
    source_name: &str,
    files_text: &str,
    extracted_text: Option<&str>,
) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    contains_ci(source_name, &needle)
        || contains_ci(files_text, &needle)
        || extracted_text.is_some_and(|text| contains_ci(text, &needle))
}

/// 检索结果条数上限的**唯一事实源**。
///
/// 此前四个宿主想出了四个值——Tauri 命令 20、桌面 MCP 20、移动 100、Web 50——而内核的截断是
/// 「按 `received_at` 倒序之后截断」，掉的永远是**最早收到**的那批。于是同一批数据、同一个
/// 查询词，一个老条目在一端「搜不到」、在另一端搜得到。这正是 [`INBOX_MATCH_CASES`] 那套
/// 跨端语料想防的分叉，只不过分叉点在条数而不在判据，语料覆盖不到。
///
/// **宿主不传这个数**：一律走 [`search_inbox_capped`](crate::store::InboxStore::search_inbox_capped)，
/// 那里收 `Option` 并在缺省时取本常量、传入时钳到本常量。TS 侧引不到 Rust const
/// （specta 与 wasm-bindgen 都不导出常量），所以让 Rust 做决定，比让前端各抄一份数字可靠。
pub const INBOX_SEARCH_LIMIT: usize = 50;

/// [`INBOX_MATCH_CASES`] 的一条用例：一组索引文本 + 一个查询词 + 期望是否命中。
#[derive(Debug, Clone, Copy)]
pub struct InboxMatchCase {
    /// 断言失败时打印，说明这条守的是什么。
    pub name: &'static str,
    pub query: &'static str,
    pub source_name: &'static str,
    pub files_text: &'static str,
    pub extracted_text: Option<&'static str>,
    pub expected: bool,
}

/// 检索命中判据的**跨端一致性语料**（conformance corpus）。
///
/// 本 crate 的单测直接对 [`inbox_matches`] 跑它；`swarmdrop-storage-sql` 把同一批数据
/// 灌进 `inbox_fts` 再走 `search_inbox`，断言同一批 `expected`。两端各写各的测试数据时，
/// 「SQL 是 `inbox_matches` 的复刻」只是一句注释；共用一份语料，它才是可执行的约束。
///
/// 覆盖面是刻意的，每一类都对应一种真实分叉：
/// - **大小写**（ASCII）——两端都要不敏感；
/// - **`%` / `_` / `\`** —— SQL 侧忘了 `escape_like` 就会把它们当通配符，凭空多出命中；
/// - **三列各自独立命中 + 多列同时命中** —— 少接一列就是少一批结果；
/// - **`extracted_text = None`** —— Web 端的常态，同一个查询词在那边应当**不**命中；
/// - **空 / 全空白 query** —— 恒不命中（SQL 侧直接短路返回空列表）。
///
/// **语料里没有「标题」列**，因为索引里也没有（理由见 [`inbox_matches`]）。删列时
/// 每条用例的承载物都改挂到 `files_text` 上，覆盖意图逐条保持不变——尤其是
/// 「2 字中文词」那条：它守的是「trigram 的 3-gram 下限不得让中文短词整体失配」，
/// 与那个词恰好出现在哪一列无关。
pub const INBOX_MATCH_CASES: &[InboxMatchCase] = &[
    InboxMatchCase {
        name: "来源名大小写不敏感",
        query: "alice",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "文件文本大小写不敏感（查询词是大写）",
        query: "A.PDF",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "文件名命中 2 字中文词（SQL 侧改用 LIKE 正为此）",
        query: "报告",
        source_name: "Alice 的工作站",
        files_text: "季度报告.pdf 季度报告.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        // 刻意用 3 字词：2 字中文那条意图归上一条用例独占，两条各守一件事。
        name: "来源名与文件文本同时命中，仍是一次命中",
        query: "设计稿",
        source_name: "设计稿中转站",
        files_text: "设计稿.fig 设计稿.fig",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "三列都不含查询词",
        query: "zzz",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: Some("正文里也没有"),
        expected: false,
    },
    InboxMatchCase {
        name: "extracted_text 独立命中（其余两列都不含）",
        query: "发票编号",
        source_name: "Bob",
        files_text: "scan-0001.pdf scan-0001.pdf",
        extracted_text: Some("发票编号 2026-07-31"),
        expected: true,
    },
    InboxMatchCase {
        name: "Web 端没有文本抽取：同一查询词在 extracted_text 缺席时不得命中",
        query: "发票编号",
        source_name: "Bob",
        files_text: "scan-0001.pdf scan-0001.pdf",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "% 是字面量：命中真的含 % 的文件名",
        query: "50%",
        source_name: "Bob",
        files_text: "预算 50% 完成.pdf 预算 50% 完成.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "% 不是通配符：a%b 不得命中 axxb（SQL 侧漏 escape_like 即在此变红）",
        query: "a%b",
        source_name: "Bob",
        files_text: "axxb.txt axxb.txt",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "_ 是字面量：命中真的含下划线的文件名",
        query: "a_b",
        source_name: "Bob",
        files_text: "data_backup.zip data_backup.zip",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "_ 不是单字符通配：a_b 不得命中 axb",
        query: "a_b",
        source_name: "Bob",
        files_text: "axb.txt axb.txt",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "反斜杠（SQL 的 ESCAPE 字符本身）当字面量命中",
        query: r"C:\Users",
        source_name: "Bob",
        files_text: r"a.txt C:\Users\a.txt",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "空查询恒不命中",
        query: "",
        source_name: "Bob",
        files_text: "合同.pdf 合同.pdf",
        extracted_text: Some("合同正文"),
        expected: false,
    },
    InboxMatchCase {
        name: "全空白查询恒不命中",
        query: "   ",
        source_name: "Bob",
        files_text: "合同.pdf 合同.pdf",
        extracted_text: Some("合同正文"),
        expected: false,
    },
];

/// 在文件文本里找首个命中子串，按字符切窗口生成片段（UTF-8 安全）。
///
/// **命中首文件名或来源名时返回 `None`，不返回片段。** 那两样在三端的条目行上本来就直接
/// 显示着（标题一行——其内容就是首文件名、「来自 {sourceName}」一行），再给一条内容相同的
/// 片段只是把同一句话说两遍——而片段行通常还带截断省略号，观感上更像是把标题截坏了。
///
/// 一个候选都不命中时同样返回 `None`：那种情况片段无从谈起（旧实现回退整个标题，理由是
/// 「搜索结果总要有一行可读的说明文本」，但标题本来就在上面那行）。
///
/// **归属判断此前收的是拼好的标题**，那让「搜索模板文字」（如多文件条目标题里的「个文件」）
/// 被误判成「用户要找的东西已在条目行上可见」。改收首文件名之后这种查询根本不该命中
/// （见 [`inbox_matches`] 为什么标题不进索引），两处是同一个根因的两面。
/// 对真实查询的行为不变：单文件条目标题就是文件名；多文件条目命中首文件名照旧不给片段、
/// 命中第二个文件照旧给。
///
/// 判据统一在这里，是因为它此前三端各判一遍：桌面无条件渲染、移动端判真、Web 端在前端
/// 比对字符串——最后那种还编码了本函数的窗口半径与省略号规则，改这里就会静默失效。
pub fn inbox_snippet(
    query: &str,
    primary_file_name: Option<&str>,
    source_name: &str,
    files: &[InboxHitFile],
) -> Option<String> {
    // 与 `inbox_matches` 同规地 trim。此前这里不 trim，只因为两个调用点都预先 trim 过——
    // 而这是个 pub 的领域规则，靠调用点纪律维系的契约迟早会破。
    let needle = query.trim().to_lowercase();
    // 首文件名与来源名只用来**判断命中归属**，不产出片段：命中它们说明用户要找的东西已经在
    // 条目行上可见了。判归属用 `contains_ci` 而不是 `snippet_window(..).is_some()`——
    // 后者会把整个窗口串（含省略号、`Vec<char>` 收集）造出来只为取一个 bool，两次。
    if primary_file_name.is_some_and(|name| contains_ci(name, &needle))
        || contains_ci(source_name, &needle)
    {
        return None;
    }
    files
        .iter()
        .find_map(|file| snippet_window(&format!("{} {}", file.name, file.relative_path), &needle))
}

/// 大小写不敏感子串——[`inbox_matches`] 与 [`inbox_snippet`] 共用的那一次折叠。
///
/// 「Rust 的 `to_lowercase` 按 Unicode 折叠、SQLite 的 `LIKE` 默认只折 ASCII」这条**刻意保留**
/// 的两端差异（理由见 [`inbox_matches`] 的文档）于是只有一个落点，不必在两处各解释一遍。
fn contains_ci(text: &str, needle_lower: &str) -> bool {
    text.to_lowercase().contains(needle_lower)
}

fn snippet_window(text: &str, needle_lower: &str) -> Option<String> {
    let hay = text.to_lowercase();
    let byte_pos = hay.find(needle_lower)?;
    let char_start = hay[..byte_pos].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let needle_len = needle_lower.chars().count();
    const CTX: usize = 16;
    let start = char_start.saturating_sub(CTX);
    let end = (char_start + needle_len + CTX).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < chars.len() {
        out.push('…');
    }
    Some(out)
}

/// 删除一条收件箱条目：可选地连文件一起删，然后软删记录。**三端共用这一份编排。**
///
/// 此前它有三份（桌面 Tauri 命令、Web `WebNode`、移动端的 **TypeScript** store），
/// 已经漂出一处可观测差异：移动端在 detail 取不到时静默跳过，另两端报错。
/// 三份实现同一段「取 detail → 逐文件删 → 软删记录」，而这段里的每一条决定
/// （顺序、失败处理、幂等）都是**领域规则**，不是平台细节——平台细节只有
/// [`FileAccess::delete_finalized_file`] 对 uri 的解释那一层。
///
/// 三条钉死的规则：
///
/// 1. **先删文件，再删记录。** 反过来的话删文件失败时记录已经没了，那份副本就再也
///    定位不到——软删项对 `list`/`search`/`detail` 一律不可见。
/// 2. **删文件失败不阻断删记录**，只记 warn。宿主可能根本不给「保留文件」这个选项
///    （Web 就不给：OPFS 副本用户无从访问，留着只是泄漏配额），那时在这里返回错误
///    就意味着用户再没有任何办法删掉这条记录。代价是那份文件成孤儿——与「删掉
///    suspended 接收会话留下的残件」是同一个已知负债，将来一并按「哪些文件真没写完」收口。
/// 3. **条目不存在则报错，不静默成功。** 只在 `delete_local_files` 时才需要 detail，
///    但那恰恰是最不该静默的分支：拿不到 detail 就等于不知道该删哪些文件，
///    静默继续会让「删了记录、文件全留下」看起来像成功。
pub async fn delete_inbox_item(
    store: &dyn crate::store::InboxStore,
    files: &dyn FileAccess,
    item_id: Uuid,
    delete_local_files: bool,
) -> AppResult<()> {
    if delete_local_files {
        let detail = store
            .get_inbox_item_detail(item_id)
            .await?
            .ok_or_else(|| AppError::Transfer("收件箱记录不存在".into()))?;
        for file in &detail.files {
            if let Err(e) = files.delete_finalized_file(&file.local_path).await {
                tracing::warn!(
                    path = %file.local_path,
                    error = %e,
                    "删除收件箱文件失败，记录仍会删除（该文件将成为孤儿）"
                );
            }
        }
    }
    store.delete_inbox_item_record(item_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(name: &'a str, relative_path: &'a str, checksum: &'a str) -> InboxFileFacts<'a> {
        InboxFileFacts {
            name,
            relative_path,
            checksum,
            size: 12,
        }
    }

    fn hit(name: &str, relative_path: &str) -> InboxHitFile {
        InboxHitFile {
            name: name.to_string(),
            relative_path: relative_path.to_string(),
        }
    }

    // ── delete_inbox_item 的编排测试 ────────────────────────────────────────
    //
    // 三条不变量此前只活在三份实现各自的注释里，没有任何测试钉着。收进领域层之后它们
    // 变成可测的纯逻辑：假 store + 假 FileAccess，不需要真的 SQLite 或 OPFS。

    use std::sync::Mutex;

    /// 记录调用顺序的假件。`log` 同时被 store 与 file access 写，用来断言「先文件后记录」。
    #[derive(Default)]
    struct DeleteSpy {
        log: Mutex<Vec<String>>,
        detail: Mutex<Option<InboxItemDetail>>,
        /// 让 `delete_finalized_file` 失败，验证「不阻断删记录」。
        fail_file_delete: bool,
    }

    impl DeleteSpy {
        fn with_files(paths: &[&str]) -> Self {
            let files = paths
                .iter()
                .enumerate()
                .map(|(i, p)| InboxItemFileEntry {
                    id: i as i32,
                    transfer_file_id: None,
                    relative_path: p.to_string(),
                    name: p.to_string(),
                    size: 1,
                    checksum: String::new(),
                    local_path: format!("/store/{p}"),
                    missing: false,
                })
                .collect();
            let spy = Self::default();
            *spy.detail.lock().unwrap() = Some(InboxItemDetail {
                item: summary_stub(),
                files,
                transfer: None,
            });
            spy
        }

        fn calls(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    fn summary_stub() -> InboxItemSummary {
        InboxItemSummary {
            id: Uuid::nil(),
            transfer_session_id: None,
            source_peer_id: String::new(),
            source_name: String::new(),
            source_kind: entity::InboxSourceKind::PairedDevice,
            content_kind: entity::InboxContentKind::Files,
            primary_file_name: None,
            item_count: 0,
            total_size: 0,
            root_path: None,
            content_hash: None,
            received_at: 0,
            last_opened_at: None,
            archived_at: None,
            deleted_at: None,
            missing: false,
        }
    }

    #[async_trait::async_trait]
    impl FileAccess for DeleteSpy {
        async fn source_metadata(
            &self,
            _s: &crate::host::FileSourceId,
        ) -> AppResult<crate::host::HostFileMetadata> {
            unimplemented!("编排不碰它")
        }
        async fn read_source_chunk(
            &self,
            _s: &crate::host::FileSourceId,
            _o: u64,
            _l: usize,
        ) -> AppResult<Vec<u8>> {
            unimplemented!("编排不碰它")
        }
        async fn create_sink(
            &self,
            _m: crate::host::HostFileMetadata,
        ) -> AppResult<crate::host::FileSinkId> {
            unimplemented!("编排不碰它")
        }
        async fn write_sink_chunk(
            &self,
            _s: &crate::host::FileSinkId,
            _o: u64,
            _d: Vec<u8>,
        ) -> AppResult<()> {
            unimplemented!("编排不碰它")
        }
        async fn finalize_sink(
            &self,
            _s: &crate::host::FileSinkId,
        ) -> AppResult<crate::host::FinalizedSink> {
            unimplemented!("编排不碰它")
        }
        async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
            self.log.lock().unwrap().push(format!("file:{uri}"));
            if self.fail_file_delete {
                return Err(AppError::Transfer("模拟删除失败".into()));
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl crate::store::InboxStore for DeleteSpy {
        async fn ensure_inbox_item_for_completed_receive_session(
            &self,
            _s: Uuid,
        ) -> AppResult<Option<InboxItemDetail>> {
            unimplemented!("编排不碰它")
        }
        async fn repair_missing_inbox_items_for_completed_receives(
            &self,
        ) -> AppResult<Vec<InboxItemDetail>> {
            unimplemented!("编排不碰它")
        }
        async fn list_inbox_items(&self, _a: bool) -> AppResult<Vec<InboxItemSummary>> {
            unimplemented!("编排不碰它")
        }
        async fn search_inbox(
            &self,
            _q: &str,
            _l: usize,
            _a: bool,
        ) -> AppResult<Vec<InboxSearchHit>> {
            unimplemented!("编排不碰它")
        }
        async fn get_inbox_item_detail(&self, _id: Uuid) -> AppResult<Option<InboxItemDetail>> {
            Ok(self.detail.lock().unwrap().clone())
        }
        async fn get_inbox_item_by_transfer_session_id(
            &self,
            _s: Uuid,
        ) -> AppResult<Option<InboxItemDetail>> {
            unimplemented!("编排不碰它")
        }
        async fn mark_inbox_item_opened(&self, _id: Uuid) -> AppResult<()> {
            unimplemented!("编排不碰它")
        }
        async fn archive_inbox_item(&self, _id: Uuid, _a: bool) -> AppResult<()> {
            unimplemented!("编排不碰它")
        }
        async fn delete_inbox_item_record(&self, _id: Uuid) -> AppResult<()> {
            self.log.lock().unwrap().push("record".to_string());
            Ok(())
        }
        async fn mark_inbox_item_file_missing(&self, _i: Uuid, _f: i32, _m: bool) -> AppResult<()> {
            unimplemented!("编排不碰它")
        }
    }

    /// 不变量 1：先删文件、再删记录。顺序反了删文件失败就再也定位不到那份副本。
    #[tokio::test]
    async fn delete_removes_files_before_record() {
        let spy = DeleteSpy::with_files(&["a.bin", "b.bin"]);
        delete_inbox_item(&spy, &spy, Uuid::nil(), true)
            .await
            .unwrap();
        assert_eq!(
            spy.calls(),
            vec!["file:/store/a.bin", "file:/store/b.bin", "record"],
            "文件必须全部先删，记录最后删"
        );
    }

    /// 不变量 2：删文件失败**不阻断**删记录——宿主可能根本不给「保留文件」选项，
    /// 在这里返回错误就意味着用户再也删不掉这条记录。
    #[tokio::test]
    async fn delete_keeps_removing_record_when_file_delete_fails() {
        let mut spy = DeleteSpy::with_files(&["a.bin"]);
        spy.fail_file_delete = true;
        delete_inbox_item(&spy, &spy, Uuid::nil(), true)
            .await
            .expect("删文件失败不该让整个操作失败");
        assert_eq!(spy.calls(), vec!["file:/store/a.bin", "record"]);
    }

    /// 不变量 3：`delete_local_files = false` 时**一个文件都不碰**，只删账本。
    #[tokio::test]
    async fn delete_record_only_never_touches_files() {
        let spy = DeleteSpy::with_files(&["a.bin"]);
        delete_inbox_item(&spy, &spy, Uuid::nil(), false)
            .await
            .unwrap();
        assert_eq!(spy.calls(), vec!["record"], "不该读 detail，也不该删文件");
    }

    /// 条目不存在 → 报错而不是静默成功。拿不到 detail 就等于不知道该删哪些文件，
    /// 静默继续会让「删了记录、文件全留下」看起来像成功。
    #[tokio::test]
    async fn delete_errors_when_item_missing() {
        let spy = DeleteSpy::default();
        let err = delete_inbox_item(&spy, &spy, Uuid::nil(), true)
            .await
            .expect_err("条目不存在应报错");
        assert!(err.to_string().contains("收件箱记录不存在"));
        assert!(spy.calls().is_empty(), "报错时不该删任何东西");
    }

    /// 首文件名只有两种结果：没有文件 → `None`，否则 → 第 0 个的 `name`。
    ///
    /// 三种展示形态（空 / 单 / 多）的判别交给各端按 `item_count` 做，领域层不再有
    /// 「标题」这个概念——这条测试守的正是它**不该**再长回来。
    #[test]
    fn primary_file_name_is_none_only_when_empty() {
        assert_eq!(inbox_primary_file_name(&[]), None);
        assert_eq!(
            inbox_primary_file_name(&[facts("a.txt", "a.txt", "sum-a")]),
            Some("a.txt".to_string())
        );
        // 多文件时取第 0 个，与 inbox_content_hash 的累加起点是同一个顺序契约。
        assert_eq!(
            inbox_primary_file_name(&[
                facts("a.txt", "a.txt", "sum-a"),
                facts("b.txt", "docs/b.txt", "sum-b"),
                facts("c.txt", "docs/c.txt", "sum-c"),
            ]),
            Some("a.txt".to_string())
        );
    }

    /// 已知向量：钉死十六进制串，防两端（SQL 与 Web）的累加顺序静默漂移。
    ///
    /// 这条测试的价值全在「期望值是常量」上——写成「两次调用结果相等」是测不出
    /// 字节序改动的。改动 hash 输入必然改这个串，届时要同步评估存量 `content_hash` 作废。
    #[test]
    fn inbox_content_hash_known_vector() {
        let files = [
            InboxFileFacts {
                name: "hello.txt",
                relative_path: "hello.txt",
                checksum: "checksum-0",
                size: 12,
            },
            InboxFileFacts {
                name: "readme.md",
                relative_path: "docs/readme.md",
                checksum: "checksum-1",
                size: 8,
            },
        ];
        assert_eq!(
            inbox_content_hash(&files),
            "d574ff2b0c617b92d1d827e0f3cf5410d2d3e5c1a165393969338c15f67c9a04"
        );
        // 空条目也有确定指纹（blake3 空输入），不是特例分支。
        assert_eq!(
            inbox_content_hash(&[]),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn files_text_joins_name_and_relative_path() {
        assert_eq!(inbox_files_text(&[]), "");
        assert_eq!(
            inbox_files_text(&[
                facts("a.txt", "a.txt", "sum-a"),
                facts("b.txt", "docs/b.txt", "sum-b"),
            ]),
            "a.txt a.txt b.txt docs/b.txt"
        );
    }

    #[test]
    fn source_kind_derived_from_origin() {
        assert!(matches!(
            inbox_source_kind(None),
            entity::InboxSourceKind::PairedDevice
        ));
        assert!(matches!(
            inbox_source_kind(Some("human")),
            entity::InboxSourceKind::PairedDevice
        ));
        assert!(matches!(
            inbox_source_kind(Some("mcp")),
            entity::InboxSourceKind::Mcp
        ));
        assert!(matches!(
            inbox_source_kind(Some("mcp:claude-desktop")),
            entity::InboxSourceKind::Mcp
        ));
    }

    /// 规范定义这一侧的一致性断言。`swarmdrop-storage-sql` 那侧灌同一批 `INBOX_MATCH_CASES`
    /// 走真实的 `search_inbox`，断言同一批 `expected`——两边都绿才叫「SQL 复刻了本函数」。
    #[test]
    fn matches_conforms_to_shared_corpus() {
        for case in INBOX_MATCH_CASES {
            assert_eq!(
                inbox_matches(
                    case.query,
                    case.source_name,
                    case.files_text,
                    case.extracted_text,
                ),
                case.expected,
                "语料用例失败: {}（query={:?}）",
                case.name,
                case.query
            );
        }
    }

    fn session(
        direction: entity::TransferDirection,
        phase: entity::TransferPhase,
        terminal_reason: Option<entity::TerminalReason>,
    ) -> entity::transfer_session::Model {
        entity::transfer_session::Model {
            session_id: Uuid::nil(),
            direction,
            peer_id: entity::PeerId("peer-123".to_string()),
            peer_name: "测试设备".into(),
            total_size: 1,
            transferred_bytes: 1,
            status: entity::SessionStatus::Completed,
            phase,
            suspended_reason: None,
            terminal_reason,
            epoch: 0,
            recoverable: false,
            source_fingerprint: None,
            started_at: 1,
            updated_at: 2,
            finished_at: Some(3),
            error_message: None,
            policy_action: None,
            policy_reason: None,
            origin: None,
            save_path: None,
        }
    }

    /// 三段合取缺一不可——每条否定用例都对应一类**不该**进收件箱的会话。
    #[test]
    fn completed_receive_requires_all_three_conditions() {
        use entity::{TerminalReason, TransferDirection, TransferPhase};

        assert!(is_completed_receive(&session(
            TransferDirection::Receive,
            TransferPhase::Terminal,
            Some(TerminalReason::Completed),
        )));
        // 发送会话：完成了也不是「收到的内容」。
        assert!(!is_completed_receive(&session(
            TransferDirection::Send,
            TransferPhase::Terminal,
            Some(TerminalReason::Completed),
        )));
        // 还在传：终态原因即便被写进去也不算数。
        assert!(!is_completed_receive(&session(
            TransferDirection::Receive,
            TransferPhase::Active,
            Some(TerminalReason::Completed),
        )));
        // 终态但失败 / 取消。
        assert!(!is_completed_receive(&session(
            TransferDirection::Receive,
            TransferPhase::Terminal,
            Some(TerminalReason::Cancelled),
        )));
        assert!(!is_completed_receive(&session(
            TransferDirection::Receive,
            TransferPhase::Terminal,
            None,
        )));
    }

    /// 片段**只为「条目行上看不见的东西」而生**：首文件名（即标题的内容）与来源名
    /// 在三端的条目行上本来就显示着，再给一条内容相同的片段只是把同一句话说两遍。
    #[test]
    fn snippet_only_for_file_hits() {
        // 命中首文件名 → 不给片段（旧实现在这里返回标题本身）。
        assert_eq!(
            inbox_snippet("合同", Some("季度合同.pdf"), "Bob", &[]),
            None
        );
        // 命中来源名 → 同理。
        assert_eq!(inbox_snippet("Bob", Some("季度合同.pdf"), "Bob", &[]), None);
        // 命中文件文本 → 这才是片段有信息量的场合：命中的东西不在条目行上。
        assert_eq!(
            inbox_snippet(
                "readme",
                Some("季度合同.pdf"),
                "Bob",
                &[hit("readme.md", "docs/readme.md")]
            ),
            Some("readme.md docs/readme.…".to_string())
        );
        // 一个候选都不命中 → 无片段可言（旧实现回退整个标题）。
        assert_eq!(inbox_snippet("zzz", Some("季度合同.pdf"), "Bob", &[]), None);
        // 首文件与其它文件同时命中时仍不给片段：条目行上那条已经可见，优先级不变。
        assert_eq!(
            inbox_snippet(
                "合同",
                Some("季度合同.pdf"),
                "Bob",
                &[hit("合同附件.pdf", "")]
            ),
            None
        );
        // 零文件条目没有首文件名：归属判断跳过它，片段照常由文件命中产出。
        // 收 `Option` 而不是「空条目传空串」，是为了让调用点能直接 `.as_deref()` 递
        // `InboxItemSummary::primary_file_name`，不必 `unwrap_or_default()`——
        // 那种转换每出现一次，就是一处「没有」与「空的」被抹平的地方。
        assert_eq!(
            inbox_snippet("readme", None, "Bob", &[hit("readme.md", "docs/readme.md")]),
            Some("readme.md docs/readme.…".to_string())
        );
    }

    /// 窗口边界：命中在正中时首尾都加 `…`，贴边时对应一侧不加。
    /// 全程按**字符**切，CJK 不会被切成半个码点（切字节会直接 panic）。
    ///
    /// 长文本挂在**非首文件**上——首文件名命中现在不产出片段（见上一个测试）。
    #[test]
    fn snippet_window_is_char_based_and_marks_truncated_sides() {
        // 命中词前后各 20 个 CJK 字符，两侧都超出 ±16 的窗口。
        let long =
            "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉合同戌亥甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未";
        let snippet = inbox_snippet("合同", Some("无关文件.txt"), "", &[hit(long, "")])
            .expect("文件文本命中应产出片段");
        assert!(snippet.starts_with('…'), "左侧被截断应带省略号");
        assert!(snippet.ends_with('…'), "右侧被截断应带省略号");
        assert!(snippet.contains("合同"));
        // ±16 字符窗口 + 命中词本身 + 两个省略号。
        assert_eq!(snippet.chars().count(), 16 + 2 + 16 + 2);

        // 命中贴着开头 → 左侧不加省略号；文本短于窗口 → 右侧也不加。
        assert_eq!(
            inbox_snippet(
                "合同",
                Some("无关文件.txt"),
                "",
                &[hit("合同扫描件", "docs")]
            ),
            Some("合同扫描件 docs".to_string())
        );
    }
}
