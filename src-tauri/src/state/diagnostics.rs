use super::AppState;
#[cfg(feature = "desktop")]
use crate::diagnostics::Diagnostics;
use crate::diagnostics::{DiagnosticStatus, EventCode};
#[cfg(feature = "desktop")]
use std::sync::Arc;
impl AppState {
    pub fn diagnostics_status(&self) -> DiagnosticStatus {
        self.diagnostics
            .lock()
            .expect("diagnostics lock poisoned")
            .as_ref()
            .map_or_else(DiagnosticStatus::default, |diagnostics| {
                diagnostics.status()
            })
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn set_diagnostics(&self, diagnostics: Arc<Diagnostics>) {
        *self.diagnostics.lock().expect("diagnostics lock poisoned") = Some(diagnostics);
        self.record_diagnostic(EventCode::Startup, 0, 0);
    }
    pub(crate) fn record_diagnostic(&self, code: EventCode, duration_ms: u64, count: u64) {
        if let Some(diagnostics) = self
            .diagnostics
            .lock()
            .expect("diagnostics lock poisoned")
            .as_ref()
        {
            diagnostics.record(code, duration_ms, count);
        }
    }
    pub(super) fn stop_diagnostics(&self) {
        if let Some(diagnostics) = self
            .diagnostics
            .lock()
            .expect("diagnostics lock poisoned")
            .as_ref()
        {
            diagnostics.record(EventCode::Shutdown, 0, 0);
            diagnostics.shutdown();
        }
    }
}
