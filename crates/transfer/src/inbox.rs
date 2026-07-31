//! 收件箱领域模型与规则（DTO + 各存储实现共用的判据）。
//!
//! 收件箱是「已接收内容索引」，与 transfer_sessions / transfer_files 的过程账本分开维护。
//! 本模块只放**数据与规则**；端口 trait [`crate::store::InboxStore`] 留在 `store.rs`
//! —— 那里是端口定义的落点，`TransferStore` 的 supertrait 组合也在那，搬走会让端口
//! 定义处读起来断成两截。
//!
//! 这里的规则由**所有存储实现共用**：SQL 侧（`swarmdrop-storage-sql`）与 Web 侧
//! （`crates/web`）各自从自己的行类型构造 [`InboxFileFacts`] 再调它们。分叉即意味着
//! 同一批文件在两端得到不同的标题、不同的内容指纹、不同的检索命中集合。
//!
//! **wasm 硬约束**：本模块只吃纯 scalar 类型，绝不出现 `entity::*::ModelEx`
//! —— 它是 `HasMany` / `HasOne` 的宿主，一旦上签名就把 sea-orm 的关系机制拖进 wasm
//! target。`From<ModelEx>` 之类的转换留在各自的存储实现里。

use uuid::Uuid;

use crate::store::TransferProjection;

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
    pub title: String,
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
    pub relative_path: String,
    pub name: String,
    pub size: i64,
    pub checksum: String,
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
    pub title: String,
    pub source_name: String,
    pub item_count: i32,
    pub root_path: Option<String>,
    pub received_at: i64,
    /// 命中所在文本的片段（在 Rust 端按子串位置切窗口生成）。
    pub snippet: String,
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
pub struct InboxFileFacts<'a> {
    pub name: &'a str,
    pub relative_path: &'a str,
    pub checksum: &'a str,
    pub size: i64,
}

/// 条目标题：0 个文件 →「空传输」，1 个 → 文件名，多个 →「X 等 N 个文件」。
pub fn inbox_title(files: &[InboxFileFacts<'_>]) -> String {
    match files {
        [] => "空传输".to_string(),
        [file] => file.name.to_string(),
        [first, ..] => format!("{} 等 {} 个文件", first.name, files.len()),
    }
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

/// 检索命中判据的**规范定义**：大小写不敏感子串，覆盖 title / source_name / files_text /
/// extracted_text 四列。
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
    title: &str,
    source_name: &str,
    files_text: &str,
    extracted_text: Option<&str>,
) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    title.to_lowercase().contains(&needle)
        || source_name.to_lowercase().contains(&needle)
        || files_text.to_lowercase().contains(&needle)
        || extracted_text.is_some_and(|text| text.to_lowercase().contains(&needle))
}

/// [`INBOX_MATCH_CASES`] 的一条用例：一组索引文本 + 一个查询词 + 期望是否命中。
#[derive(Debug, Clone, Copy)]
pub struct InboxMatchCase {
    /// 断言失败时打印，说明这条守的是什么。
    pub name: &'static str,
    pub query: &'static str,
    pub title: &'static str,
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
/// - **四列各自独立命中 + 多列同时命中** —— 少接一列就是少一批结果；
/// - **`extracted_text = None`** —— Web 端的常态，同一个查询词在那边应当**不**命中；
/// - **空 / 全空白 query** —— 恒不命中（SQL 侧直接短路返回空列表）。
pub const INBOX_MATCH_CASES: &[InboxMatchCase] = &[
    InboxMatchCase {
        name: "来源名大小写不敏感",
        query: "alice",
        title: "季度报告",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "文件文本大小写不敏感（查询词是大写）",
        query: "A.PDF",
        title: "季度报告",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "标题命中 2 字中文词（SQL 侧改用 LIKE 正为此）",
        query: "报告",
        title: "季度报告",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "标题与文件文本同时命中，仍是一次命中",
        query: "合同",
        title: "合同.pdf",
        source_name: "Bob",
        files_text: "合同.pdf 合同.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "四列都不含查询词",
        query: "zzz",
        title: "季度报告",
        source_name: "Alice 的工作站",
        files_text: "a.pdf a.pdf",
        extracted_text: Some("正文里也没有"),
        expected: false,
    },
    InboxMatchCase {
        name: "extracted_text 独立命中（其余三列都不含）",
        query: "发票编号",
        title: "scan-0001.pdf",
        source_name: "Bob",
        files_text: "scan-0001.pdf scan-0001.pdf",
        extracted_text: Some("发票编号 2026-07-31"),
        expected: true,
    },
    InboxMatchCase {
        name: "Web 端没有文本抽取：同一查询词在 extracted_text 缺席时不得命中",
        query: "发票编号",
        title: "scan-0001.pdf",
        source_name: "Bob",
        files_text: "scan-0001.pdf scan-0001.pdf",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "% 是字面量：命中真的含 % 的标题",
        query: "50%",
        title: "预算 50% 完成.pdf",
        source_name: "Bob",
        files_text: "预算 50% 完成.pdf 预算 50% 完成.pdf",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "% 不是通配符：a%b 不得命中 axxb（SQL 侧漏 escape_like 即在此变红）",
        query: "a%b",
        title: "axxb.txt",
        source_name: "Bob",
        files_text: "axxb.txt axxb.txt",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "_ 是字面量：命中真的含下划线的文件名",
        query: "a_b",
        title: "data_backup.zip",
        source_name: "Bob",
        files_text: "data_backup.zip data_backup.zip",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "_ 不是单字符通配：a_b 不得命中 axb",
        query: "a_b",
        title: "axb.txt",
        source_name: "Bob",
        files_text: "axb.txt axb.txt",
        extracted_text: None,
        expected: false,
    },
    InboxMatchCase {
        name: "反斜杠（SQL 的 ESCAPE 字符本身）当字面量命中",
        query: r"C:\Users",
        title: r"C:\Users\a.txt",
        source_name: "Bob",
        files_text: r"a.txt C:\Users\a.txt",
        extracted_text: None,
        expected: true,
    },
    InboxMatchCase {
        name: "空查询恒不命中",
        query: "",
        title: "合同.pdf",
        source_name: "Bob",
        files_text: "合同.pdf 合同.pdf",
        extracted_text: Some("合同正文"),
        expected: false,
    },
    InboxMatchCase {
        name: "全空白查询恒不命中",
        query: "   ",
        title: "合同.pdf",
        source_name: "Bob",
        files_text: "合同.pdf 合同.pdf",
        extracted_text: Some("合同正文"),
        expected: false,
    },
];

/// 在标题 / 来源名 / 文件文本里找首个命中子串，按字符切窗口生成片段（UTF-8 安全）。
///
/// 候选顺序即优先级：标题 → 来源名 → 各文件的 `"{name} {relative_path}"`。
/// 一个都没命中时回退整个标题（搜索结果总要有一行可读的说明文本）。
pub fn inbox_snippet(
    query: &str,
    title: &str,
    source_name: &str,
    files: &[InboxHitFile],
) -> String {
    let needle = query.to_lowercase();
    let mut candidates: Vec<String> = vec![title.to_string(), source_name.to_string()];
    for file in files {
        candidates.push(format!("{} {}", file.name, file.relative_path));
    }
    for text in &candidates {
        if let Some(snippet) = snippet_window(text, &needle) {
            return snippet;
        }
    }
    title.to_string()
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

    #[test]
    fn title_covers_empty_single_and_multi() {
        assert_eq!(inbox_title(&[]), "空传输");
        assert_eq!(
            inbox_title(&[facts("a.txt", "a.txt", "sum-a")]),
            "a.txt".to_string()
        );
        assert_eq!(
            inbox_title(&[
                facts("a.txt", "a.txt", "sum-a"),
                facts("b.txt", "docs/b.txt", "sum-b"),
                facts("c.txt", "docs/c.txt", "sum-c"),
            ]),
            "a.txt 等 3 个文件"
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
                    case.title,
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

    #[test]
    fn snippet_prefers_title_then_source_then_files() {
        assert_eq!(
            inbox_snippet("合同", "季度合同.pdf", "Bob", &[]),
            "季度合同.pdf"
        );
        assert_eq!(inbox_snippet("Bob", "季度合同.pdf", "Bob", &[]), "Bob");
        assert_eq!(
            inbox_snippet(
                "readme",
                "季度合同.pdf",
                "Bob",
                &[hit("readme.md", "docs/readme.md")]
            ),
            "readme.md docs/readme.…"
        );
        // 一个候选都不命中 → 回退整个标题。
        assert_eq!(
            inbox_snippet("zzz", "季度合同.pdf", "Bob", &[]),
            "季度合同.pdf"
        );
    }

    /// 窗口边界：命中在正中时首尾都加 `…`，贴边时对应一侧不加。
    /// 全程按**字符**切，CJK 不会被切成半个码点（切字节会直接 panic）。
    #[test]
    fn snippet_window_is_char_based_and_marks_truncated_sides() {
        // 命中词前后各 20 个 CJK 字符，两侧都超出 ±16 的窗口。
        let long =
            "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉合同戌亥甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未";
        let snippet = inbox_snippet("合同", long, "", &[]);
        assert!(snippet.starts_with('…'), "左侧被截断应带省略号");
        assert!(snippet.ends_with('…'), "右侧被截断应带省略号");
        assert!(snippet.contains("合同"));
        // ±16 字符窗口 + 命中词本身 + 两个省略号。
        assert_eq!(snippet.chars().count(), 16 + 2 + 16 + 2);

        // 命中贴着开头 → 左侧不加省略号；文本短于窗口 → 右侧也不加。
        assert_eq!(inbox_snippet("合同", "合同扫描件", "", &[]), "合同扫描件");
    }
}
