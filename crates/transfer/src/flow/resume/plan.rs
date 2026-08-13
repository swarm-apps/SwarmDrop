//! 断点续传纯构造 / 计划生成（无 `self` / `TransferManager` 依赖）。
//!
//! manifest / checkpoint / fetch_plan / 续传 state 等的派生，均只读 entity model +
//! protocol 类型；与 [`super::validation`] 互相引用（plan 用其 `validate_checkpoint`，
//! validation 用本模块 `build_resume_manifest`），同 crate 双向模块引用无碍。

use std::collections::HashMap;

use crate::host::FileSourceId;
use crate::manager::{PreparedFile, ResumeFileInfo};
use crate::protocol::{FileCheckpoint, FileInfo, FileRange, ResumeRejectReason, ResumeReport};
use crate::{AppError, AppResult};
use crate::{CHUNK_SIZE, calc_total_chunks};

use super::validation::{resume_reject_message, validate_checkpoint};

/// 从文件记录构造 checkpoint（过渡近似：transferred_bytes 作单个连续 range；
/// 精确 bitmap→ranges 在轮 7 数据面落实）。
pub(crate) fn build_resume_checkpoint(
    files: &[entity::transfer_file::Model],
) -> Vec<FileCheckpoint> {
    files
        .iter()
        .map(|f| FileCheckpoint {
            file_id: f.file_id as u32,
            completed_ranges: {
                let ranges = crate::store::parse_completed_ranges(&f.completed_ranges);
                if !ranges.is_empty() {
                    ranges
                } else if f.transferred_bytes > 0 {
                    vec![(0, f.transferred_bytes as u64)]
                } else {
                    vec![]
                }
            },
        })
        .collect()
}

pub(crate) fn build_resume_manifest(files: &[entity::transfer_file::Model]) -> Vec<FileInfo> {
    files.iter().map(FileInfo::from).collect()
}

pub(crate) fn build_fetch_plan_from_files(
    files: &[entity::transfer_file::Model],
) -> AppResult<Vec<FileRange>> {
    let manifest = build_resume_manifest(files);
    let checkpoint = build_resume_checkpoint(files);
    build_fetch_plan(&manifest, &checkpoint)
}

pub(crate) fn build_fetch_plan_from_report(report: &ResumeReport) -> AppResult<Vec<FileRange>> {
    build_fetch_plan(&report.files, &report.checkpoint)
}

fn build_fetch_plan(
    manifest: &[FileInfo],
    checkpoint: &[FileCheckpoint],
) -> AppResult<Vec<FileRange>> {
    validate_checkpoint(manifest, checkpoint).map_err(|_| {
        AppError::Transfer(resume_reject_message(&ResumeRejectReason::CheckpointInvalid).into())
    })?;

    let mut plan = Vec::new();
    for file in manifest {
        let mut cursor = 0u64;
        if let Some(cp) = checkpoint.iter().find(|item| item.file_id == file.file_id) {
            for &(offset, length) in &cp.completed_ranges {
                if cursor < offset {
                    plan.push(FileRange {
                        file_id: file.file_id,
                        offset: cursor,
                        length: offset - cursor,
                    });
                }
                cursor = offset + length;
            }
        }
        if cursor < file.size {
            plan.push(FileRange {
                file_id: file.file_id,
                offset: cursor,
                length: file.size - cursor,
            });
        }
    }
    Ok(plan)
}

pub(crate) fn next_resume_epoch(local_epoch: i64, peer_epoch: i64) -> i64 {
    local_epoch.max(peer_epoch) + 1
}

/// 发送侧续传的进度基线：`file_id → (chunks_done, bytes_done)`，喂给
/// [`ProgressTracker::init_files_with_resume`](crate::progress::ProgressTracker::init_files_with_resume)。
///
/// # 基线来自 fetch_plan 的补集，不来自 `transferred_bytes`
///
/// `transfer_file.transferred_bytes` 只在**优雅**路径写（sender 的 `on_completed` /
/// `on_interrupted`、`pause_send`、`handle_pause_impl` 四处）。进程被杀——Android 后台回收
/// 是常态——那一列就恒为 0，于是续传的进度条从 0 爬到「已传部分 / 总量」处便宣告完成，
/// 用户看到的是「又从头传了一遍」。对照组是接收侧：它从每 10 块落库的 bitmap 现算
/// （`actor::receiver`），掉电也在。
///
/// fetch_plan 是「对端还缺哪些字节」的权威事实（发起侧由对端 report 推导、被动应答侧
/// 直接来自对端），取补集即得本端已经送达的部分，不依赖任何优雅退出。
///
/// # 必须数块，不能拿字节除 `CHUNK_SIZE`
///
/// `bytes_done / CHUNK_SIZE` 在「短尾块已收到而中间有洞」时**少算一块**：4 块的文件缺中间
/// 一块时 `bytes_done = 2·CHUNK + tail`，floor 得 2，真实是 3。少的那一块让
/// [`update_file_chunk`](crate::progress::ProgressTracker::update_file_chunk) 永远到不了
/// `chunks_done >= total_chunks`，该文件**永不转 Completed**、`completed_files` 卡在 n−1。
/// 所以这里按**块索引**计数，与接收侧 `checkpoint::count_completed_in_bitmap` 同口径。
///
/// # 这里不能假设 plan 已对齐
///
/// 对齐是**对端**在 `validate_resume_commit` → `validate_fetch_plan` 里查的；发起侧
/// （`initiate_resume` 里 commit 之前那一步）算基线时，plan 还没被任何人校验过——
/// 被动应答侧则相反，那边 `validate_resume_commit` 已先行。故块数按
/// `floor(offset)…ceil(end)` 取：非对齐输入只会把 missing **多算**、基线估低，绝不会
/// 反过来把没送到的块认成已完成。（非对齐的计划本来也走不到完成——`bao::encode_proof`
/// 在第一块就会拒，所以「估低」不会真的害到谁。）
///
/// # 全缺的文件也要进表，值为 `(0, 0)`
///
/// 对 `init_files_with_resume` 而言「缺项」与 `(0, 0)` 等价，所以这里曾用 `filter_map`
/// 把它们滤掉。但基线现在还有两个消费者——[`apply_sender_resume_baseline`] 用它回写 DB
/// 和回填内存快照——对**它们**不等价：滤掉就等于「不写」，那一列会保留优雅暂停时留下的
/// 旧值。于是 `dispatch(ResumeCommitted)` 先按旧值 emit 一份高报的 projection，紧接着
/// tracker 的第一条进度把它拽回真值，进度条当场倒退。
///
/// [`apply_sender_resume_baseline`]: super::TransferManager::apply_sender_resume_baseline
pub(crate) fn build_sender_resume_state_from_plan(
    files: &[entity::transfer_file::Model],
    fetch_plan: &[FileRange],
) -> HashMap<u32, (u32, u64)> {
    files
        .iter()
        .map(|f| {
            let file_id = f.file_id as u32;
            let file_size = f.size as u64;
            let (missing_chunks, missing_bytes) =
                count_missing_from_plan(file_id, file_size, fetch_plan);

            let chunks_done = calc_total_chunks(file_size).saturating_sub(missing_chunks);
            let bytes_done = file_size.saturating_sub(missing_bytes);
            (file_id, (chunks_done, bytes_done))
        })
        .collect()
}

/// 单个文件在 fetch_plan 里的缺口：`(缺的块数, 缺的字节数)`。
///
/// 「为什么数块而不是拿字节除 `CHUNK_SIZE`」「为什么不能假设 plan 已对齐」两条判据写在
/// 唯一调用方 [`build_sender_resume_state_from_plan`] 的文档里。
fn count_missing_from_plan(file_id: u32, file_size: u64, fetch_plan: &[FileRange]) -> (u32, u64) {
    let chunk_size = CHUNK_SIZE as u64;
    let mut missing_chunks: u32 = 0;
    let mut missing_bytes: u64 = 0;
    for range in fetch_plan.iter().filter(|r| r.file_id == file_id) {
        // 越界输入夹到文件边界内：越界的 plan 会在别处被拒，这里只保证算术不撒谎。
        let start = range.offset.min(file_size);
        let end = range.offset.saturating_add(range.length).min(file_size);
        if end <= start {
            continue;
        }
        missing_bytes = missing_bytes.saturating_add(end - start);
        let first_chunk = (start / chunk_size) as u32;
        // `end` 已夹进 `[0, file_size]`，故 `ceil(end)` 天然 ≤ `calc_total_chunks(file_size)`，
        // 不必再夹一次上界。
        let last_chunk = end.div_ceil(chunk_size) as u32;
        missing_chunks = missing_chunks.saturating_add(last_chunk.saturating_sub(first_chunk));
    }
    (missing_chunks, missing_bytes)
}

pub(crate) fn build_resume_file_infos(
    files: &[entity::transfer_file::Model],
) -> (Vec<ResumeFileInfo>, i64) {
    let mut infos = Vec::with_capacity(files.len());
    let mut transferred_bytes: i64 = 0;
    for f in files {
        infos.push(ResumeFileInfo {
            file_id: f.file_id,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
        });
        transferred_bytes += f.transferred_bytes;
    }
    (infos, transferred_bytes)
}

/// 从 DB 行重建 `FileInfo` 与接收侧的分块位图。
///
/// # 载入时必须把「位图完整却从未发布」的行降级
///
/// 接收侧的核心不变量是**「位图完整 ⟺ 该文件已发布」**——收齐即发布，且末块刻意不刷
/// checkpoint，完整位图只由 `mark_file_completed` 在发布成功后写。
///
/// 但**存量数据破坏这条不变量**：旧实现在每个文件的末块就刷完整位图，而发布推迟到会话
/// 结束统一做。于是「末块落库后、会话结束前」中断的行，在库里就是「位图完整 + `local_path`
/// 为空」。原样载入的话，续传的 `first_missing_range` 会跳过它、`fetch_plan` 不含它、
/// 于是永远不会有块到达来触发发布，而 `ensure_files_complete` 又判它完成——
/// 会话报成功，文件却留在暂存里，`local_path` 永远是 NULL。**静默丢文件。**
///
/// `local_path` 是发布的唯一凭证（只由 `mark_file_completed` 写），所以这里用它做判据：
/// 未发布的行清掉最后一块，让它重新走一遍「收齐 → 发布」。代价是重传一个 chunk，
/// 换回不变量成立。
pub(crate) fn build_file_infos_and_bitmaps(
    files: &[entity::transfer_file::Model],
) -> (Vec<FileInfo>, HashMap<u32, Vec<u8>>) {
    let mut file_infos = Vec::with_capacity(files.len());
    let mut bitmaps = HashMap::with_capacity(files.len());
    for f in files {
        let fid = f.file_id as u32;
        file_infos.push(FileInfo::from(f));

        let mut bitmap = f.completed_chunks.clone();
        if f.local_path.is_none() {
            let total_chunks = calc_total_chunks(f.size as u64);
            crate::actor::checkpoint::clear_chunk_completed(
                &mut bitmap,
                total_chunks.saturating_sub(1),
            );
        }
        bitmaps.insert(fid, bitmap);
    }
    (file_infos, bitmaps)
}

/// 从 DB 重建 PreparedFile，不做 fs 探测（让 sender 端推送时读取源文件失败再报错）
pub(crate) fn build_prepared_files_from_db(
    files: &[entity::transfer_file::Model],
) -> Vec<PreparedFile> {
    files
        .iter()
        .map(|f| PreparedFile {
            file_id: f.file_id as u32,
            name: f.name.clone(),
            relative_path: f.relative_path.clone(),
            source_id: FileSourceId(f.source_path.clone().unwrap_or_default()),
            size: f.size as u64,
            checksum: f.checksum.clone(),
            // outboard 随会话落库。这里只把 DB 的 Option 摊平，**不判可用性**——
            // 唯一的调用者 `build_sender_actor_for_resume` 紧接着就用
            // `bao::is_outboard_usable` 统一过一道关（空值、旧会话、格式作废的存量三
            // 种情况在那里合并处理），判据放两处只会漂移。
            outboard: f.outboard.clone().unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: u64 = CHUNK_SIZE as u64;

    /// `transferred_bytes` 一律置 0——被杀的进程留下的就是这个样子，正是本函数要绕开的那一列。
    fn file_model(file_id: i32, size: u64) -> entity::transfer_file::Model {
        entity::transfer_file::Model {
            id: file_id,
            session_id: uuid::Uuid::nil(),
            file_id,
            name: format!("f{file_id}.bin"),
            relative_path: format!("f{file_id}.bin"),
            size: size as i64,
            checksum: "checksum".to_string(),
            status: entity::FileStatus::Pending,
            transferred_bytes: 0,
            total_chunks: calc_total_chunks(size) as i32,
            completed_chunks: vec![],
            completed_ranges: String::new(),
            source_path: Some(format!("/tmp/f{file_id}.bin")),
            local_path: None,
            local_dir: None,
            outboard: None,
        }
    }

    fn range(file_id: u32, offset: u64, length: u64) -> FileRange {
        FileRange {
            file_id,
            offset,
            length,
        }
    }

    /// 中间有洞、短尾块已收到：块数必须按块索引数，字节数按 plan 的补集算。
    ///
    /// 这是「拿字节除 `CHUNK_SIZE`」会翻车的形状——floor 只给 2，真实是 3，
    /// 差的那一块会让文件永不转 Completed。
    #[test]
    fn baseline_counts_chunks_not_bytes_when_plan_has_a_hole() {
        let size = CHUNK * 3 + 777; // 4 块，末块 777 字节
        let files = vec![file_model(1, size)];
        // 只缺中间的第 1 块（索引从 0 起），块 0 / 2 / 3 都已送达。
        let plan = vec![range(1, CHUNK, CHUNK)];

        let state = build_sender_resume_state_from_plan(&files, &plan);

        assert_eq!(
            state.get(&1).copied(),
            Some((3, size - CHUNK)),
            "4 块缺 1 块 ⇒ 已完成 3 块；字节数取 plan 的补集"
        );
    }

    /// 完全不在计划里的文件 = 对端已收齐 ⇒ 必须给出完整块数，否则它永远停在 Pending。
    ///
    /// 旧实现（读 `transferred_bytes`）在进程被杀后对这种文件返回 `None`，
    /// `init_files_with_resume` 便按 `(0, 0)` 当作一块没传。
    #[test]
    fn baseline_marks_files_absent_from_plan_as_complete() {
        let done_size = CHUNK * 2;
        let pending_size = CHUNK * 3;
        let files = vec![file_model(1, done_size), file_model(2, pending_size)];
        // 计划里只有文件 2 的后两块。
        let plan = vec![range(2, CHUNK, CHUNK * 2)];

        let state = build_sender_resume_state_from_plan(&files, &plan);

        assert_eq!(
            state.get(&1).copied(),
            Some((calc_total_chunks(done_size), done_size)),
            "不在 plan 里 ⇒ 对端已收齐，基线必须是满的"
        );
        assert_eq!(
            state.get(&2).copied(),
            Some((1, CHUNK)),
            "3 块缺后 2 块 ⇒ 已完成 1 块"
        );
    }

    /// 全缺的文件必须以 `(0, 0)` **进表**，不能被滤掉。
    ///
    /// 对 tracker 而言缺项与 `(0, 0)` 等价，所以这里一度用 `filter_map` 滤掉它们。但
    /// `apply_sender_resume_baseline` 拿同一张表回写 DB 与内存快照，滤掉就等于「不写」：
    /// 优雅暂停留下的旧值原样留在 `transferred_bytes` 里，续传时 projection 先高报、
    /// 再被 tracker 的第一条进度拽回真值，进度条当场倒退。
    ///
    /// 它红了就说明那个倒退回来了——而这条路径没有别的护栏。
    #[test]
    fn baseline_includes_fully_missing_files_as_zero() {
        let size = CHUNK * 2;
        let files = vec![file_model(1, size), file_model(2, size)];
        // 文件 1 整个都缺（对端 checkpoint 还是空），文件 2 已收齐（不在 plan 里）。
        let plan = vec![range(1, 0, size)];

        let state = build_sender_resume_state_from_plan(&files, &plan);

        assert_eq!(
            state.get(&1).copied(),
            Some((0, 0)),
            "全缺的文件要以 (0,0) 进表——回写侧靠它把 DB 那列**改小到 0**"
        );
        assert_eq!(
            state.len(),
            files.len(),
            "每个文件都该有条目，回写才能覆盖全部文件"
        );
    }
}
