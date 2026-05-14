use entity::SaveLocation;
use tauri::{AppHandle, Emitter};

use crate::events;

pub use swarmdrop_core::transfer::progress::{
    FileDesc, ProgressTracker, TransferCompleteEvent, TransferDbErrorEvent, TransferDirection,
    TransferFailedEvent, TransferPausedEvent, TransferResumedEvent, TransferResumedFileInfo,
};

pub trait ProgressTrackerTauriExt {
    fn emit_progress(&mut self, app: &AppHandle);
    fn emit_complete(&self, app: &AppHandle, save_location: Option<SaveLocation>);
    fn emit_failed(&self, app: &AppHandle, error: String);
}

impl ProgressTrackerTauriExt for ProgressTracker {
    fn emit_progress(&mut self, app: &AppHandle) {
        if let Some(event) = self.progress_event() {
            let _ = app.emit(events::TRANSFER_PROGRESS, &event);
        }
    }

    fn emit_complete(&self, app: &AppHandle, save_location: Option<SaveLocation>) {
        let event = self.complete_event(save_location);
        let _ = app.emit(events::TRANSFER_COMPLETE, &event);
    }

    fn emit_failed(&self, app: &AppHandle, error: String) {
        let event = self.failed_event(error);
        let _ = app.emit(events::TRANSFER_FAILED, &event);
    }
}
