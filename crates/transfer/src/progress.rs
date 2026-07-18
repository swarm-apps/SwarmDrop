//! 文件传输进度模型和计算器。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::host::CoreSaveLocation;
use serde::Serialize;
use uuid::Uuid;

use super::calc_total_chunks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum RuntimeTransferDirection {
    Send,
    Receive,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum FileTransferStatus {
    Pending,
    Transferring,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileProgressInfo {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub status: FileTransferStatus,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub chunks_done: u32,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub total_chunks: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub total_files: usize,
    pub completed_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed: f64,
    pub eta: Option<f64>,
    pub files: Vec<FileProgressInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferCompleteEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    pub save_location: Option<CoreSaveLocation>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferFailedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferPausedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferResumedEvent {
    pub session_id: Uuid,
    pub direction: RuntimeTransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub files: Vec<TransferResumedFileInfo>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferResumedFileInfo {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferDbErrorEvent {
    pub session_id: Uuid,
    pub message: String,
}

/// 对方接受 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferAcceptedEvent {
    pub session_id: Uuid,
}

/// 对方拒绝 Offer 的事件 payload
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferRejectedEvent {
    pub session_id: Uuid,
    pub reason: Option<crate::protocol::OfferRejectReason>,
}

/// `prepare_send` 的 hash 进度事件
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PrepareProgressEvent {
    /// 用于区分并发 prepare（多用户在不同会话同时调用）
    pub prepared_id: Uuid,
    /// 当前正在 hash 的文件名
    pub current_file: String,
    /// 已完成 hash 的文件数
    pub completed_files: u32,
    /// 总文件数
    pub total_files: u32,
    /// 累积已 hash 的字节数（所有文件）
    pub bytes_hashed: u64,
    /// 总字节数（所有文件）
    pub total_bytes: u64,
}

pub struct FileDesc {
    pub file_id: u32,
    pub name: String,
    pub size: u64,
}

pub struct ProgressTracker {
    session_id: Uuid,
    direction: RuntimeTransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    total_files: usize,
    completed_files: usize,
    files: Vec<FileProgressInfo>,
    started_at: Instant,
    samples: VecDeque<(Instant, u64)>,
    last_emit: Option<Instant>,
}

const THROTTLE_INTERVAL: Duration = Duration::from_millis(200);
const SPEED_WINDOW: Duration = Duration::from_secs(3);

impl ProgressTracker {
    pub fn new(
        session_id: Uuid,
        direction: RuntimeTransferDirection,
        total_bytes: u64,
        total_files: usize,
    ) -> Self {
        Self {
            session_id,
            direction,
            total_bytes,
            transferred_bytes: 0,
            total_files,
            completed_files: 0,
            files: Vec::new(),
            started_at: Instant::now(),
            samples: VecDeque::new(),
            last_emit: None,
        }
    }

    pub fn init_files_with_resume(
        &mut self,
        file_descs: &[FileDesc],
        resume_state: &std::collections::HashMap<u32, (u32, u64)>,
    ) {
        self.files = file_descs
            .iter()
            .map(|f| {
                let total_chunks = calc_total_chunks(f.size);
                let (chunks_done, transferred) =
                    resume_state.get(&f.file_id).copied().unwrap_or((0, 0));
                let status = if chunks_done >= total_chunks {
                    FileTransferStatus::Completed
                } else if chunks_done > 0 {
                    FileTransferStatus::Transferring
                } else {
                    FileTransferStatus::Pending
                };
                FileProgressInfo {
                    file_id: f.file_id,
                    name: f.name.clone(),
                    size: f.size,
                    transferred,
                    status,
                    chunks_done,
                    total_chunks,
                }
            })
            .collect();

        self.completed_files = self
            .files
            .iter()
            .filter(|f| f.status == FileTransferStatus::Completed)
            .count();
        self.transferred_bytes = self.files.iter().map(|f| f.transferred).sum();
    }

    pub fn update_file_chunk(&mut self, file_id: u32, chunk_bytes: u64) {
        if let Some(f) = self.files.iter_mut().find(|f| f.file_id == file_id) {
            if f.status == FileTransferStatus::Completed {
                return;
            }
            if f.status == FileTransferStatus::Pending {
                f.status = FileTransferStatus::Transferring;
            }
            f.transferred += chunk_bytes;
            f.chunks_done += 1;
            if f.chunks_done >= f.total_chunks {
                f.status = FileTransferStatus::Completed;
                f.transferred = f.size;
                self.completed_files += 1;
            }
        }
    }

    pub fn set_file_transferring(&mut self, file_id: u32) {
        if let Some(f) = self.files.iter_mut().find(|f| f.file_id == file_id)
            && f.status == FileTransferStatus::Pending
        {
            f.status = FileTransferStatus::Transferring;
        }
    }

    pub fn transferred_bytes(&self) -> u64 {
        self.transferred_bytes
    }

    pub fn get_file_progress(&self) -> Vec<(u32, u32, u64)> {
        self.files
            .iter()
            .map(|f| (f.file_id, f.chunks_done, f.transferred))
            .collect()
    }

    pub fn add_bytes(&mut self, bytes: u64) {
        self.transferred_bytes += bytes;
        let now = Instant::now();
        self.samples.push_back((now, self.transferred_bytes));

        let cutoff = now - SPEED_WINDOW;
        while self.samples.front().is_some_and(|(t, _)| *t < cutoff) {
            self.samples.pop_front();
        }
    }

    pub fn speed(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let (t_first, b_first) = self.samples.front().unwrap();
        let (t_last, b_last) = self.samples.back().unwrap();
        let elapsed = t_last.duration_since(*t_first).as_secs_f64();
        if elapsed < 0.001 {
            return 0.0;
        }
        (b_last - b_first) as f64 / elapsed
    }

    pub fn eta(&self) -> Option<f64> {
        let speed = self.speed();
        if speed < 1.0 {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.transferred_bytes);
        Some(remaining as f64 / speed)
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn progress_event(&mut self) -> Option<TransferProgressEvent> {
        let now = Instant::now();
        if self
            .last_emit
            .is_some_and(|last| now.duration_since(last) < THROTTLE_INTERVAL)
        {
            return None;
        }
        self.last_emit = Some(now);

        Some(TransferProgressEvent {
            session_id: self.session_id,
            direction: self.direction,
            total_files: self.total_files,
            completed_files: self.completed_files,
            total_bytes: self.total_bytes,
            transferred_bytes: self.transferred_bytes,
            speed: self.speed(),
            eta: self.eta(),
            files: self.files.clone(),
        })
    }

    pub fn complete_event(&self, save_location: Option<CoreSaveLocation>) -> TransferCompleteEvent {
        TransferCompleteEvent {
            session_id: self.session_id,
            direction: self.direction,
            total_bytes: self.transferred_bytes,
            elapsed_ms: self.elapsed_ms(),
            save_location,
        }
    }

    pub fn failed_event(&self, error: String) -> TransferFailedEvent {
        TransferFailedEvent {
            session_id: self.session_id,
            direction: self.direction,
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn progress_event_should_include_file_and_byte_progress() {
        let session_id = Uuid::new_v4();
        let mut tracker = ProgressTracker::new(session_id, RuntimeTransferDirection::Send, 10, 1);
        tracker.init_files_with_resume(
            &[FileDesc {
                file_id: 1,
                name: "a.txt".to_string(),
                size: 10,
            }],
            &HashMap::new(),
        );

        tracker.add_bytes(10);
        tracker.update_file_chunk(1, 10);

        let event = tracker.progress_event().expect("first event is emitted");
        assert_eq!(event.session_id, session_id);
        assert_eq!(event.transferred_bytes, 10);
        assert_eq!(event.completed_files, 1);
        assert_eq!(event.files[0].status, FileTransferStatus::Completed);
    }

    #[test]
    fn resumed_progress_should_restore_completed_file_state() {
        let mut resume_state = HashMap::new();
        resume_state.insert(7, (1, 5));

        let mut tracker =
            ProgressTracker::new(Uuid::new_v4(), RuntimeTransferDirection::Receive, 5, 1);
        tracker.init_files_with_resume(
            &[FileDesc {
                file_id: 7,
                name: "resume.bin".to_string(),
                size: 5,
            }],
            &resume_state,
        );

        let event = tracker.progress_event().expect("first event is emitted");
        assert_eq!(event.transferred_bytes, 5);
        assert_eq!(event.completed_files, 1);
        assert_eq!(event.files[0].status, FileTransferStatus::Completed);
    }
}
