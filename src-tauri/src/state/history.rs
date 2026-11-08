use super::{now_ms, storage_code, AppState};
use crate::{
    history::{HistorySettings, HistoryView},
    model::DiagnosticCode,
    storage::StorageError,
};

fn history_error(error: StorageError) -> DiagnosticCode {
    match error {
        StorageError::InvalidState => DiagnosticCode::InvalidHistoryRequest,
        other => storage_code(other),
    }
}
impl AppState {
    pub fn history(&self, range_hours: u16) -> Result<HistoryView, DiagnosticCode> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .history(range_hours, now_ms())
            .map_err(history_error)
    }
    pub fn history_settings(&self) -> Result<HistorySettings, DiagnosticCode> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .history_settings()
            .map_err(history_error)
    }
    pub fn set_history_retention(&self, days: u16) -> Result<HistorySettings, DiagnosticCode> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .set_history_retention(days, now_ms())
            .map_err(history_error)
    }
    pub fn clear_history(&self, confirmed: bool) -> Result<(), DiagnosticCode> {
        if !confirmed {
            return Err(DiagnosticCode::HistoryConfirmationRequired);
        }
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .clear_history()
            .map_err(history_error)
    }
}
