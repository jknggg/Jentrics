use super::AppState;
use crate::{model::DiagnosticCode, storage::PreferenceKey};
use serde::{de::DeserializeOwned, Serialize};
impl AppState {
    pub(super) fn load_preference<T: DeserializeOwned>(
        &self,
        key: PreferenceKey,
    ) -> Result<Option<T>, DiagnosticCode> {
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_ref()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .read_preference(key)
            .map_err(super::storage_code)?
            .map(|v| serde_json::from_value(v).map_err(|_| DiagnosticCode::StorageCorrupt))
            .transpose()
    }
    pub(super) fn save_preference<T: Serialize>(
        &self,
        key: PreferenceKey,
        value: &T,
    ) -> Result<(), DiagnosticCode> {
        let value = serde_json::to_value(value).map_err(|_| DiagnosticCode::StorageCorrupt)?;
        self.storage
            .lock()
            .expect("storage lock poisoned")
            .as_mut()
            .ok_or(DiagnosticCode::StorageUnavailable)?
            .write_preference(key, &value)
            .map_err(super::storage_code)
    }
}
