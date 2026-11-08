use super::{Storage, StorageError};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
#[derive(Debug, Clone, Copy)]
pub enum PreferenceKey {
    Monitor,
    Alerts,
    AlertSeen,
    Telemetry,
    Prices,
    Display,
}
impl PreferenceKey {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Monitor => "monitor",
            Self::Alerts => "alerts",
            Self::AlertSeen => "alertSeen",
            Self::Telemetry => "telemetry",
            Self::Prices => "prices",
            Self::Display => "display",
        }
    }
}
impl Storage {
    pub fn read_preference(&self, key: PreferenceKey) -> Result<Option<Value>, StorageError> {
        let encoded: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM preferences WHERE key=?1",
                [key.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| StorageError::Corrupt)?;
        encoded
            .map(|v| {
                if v.len() > 65536 {
                    return Err(StorageError::Corrupt);
                }
                serde_json::from_str(&v).map_err(|_| StorageError::Corrupt)
            })
            .transpose()
    }
    pub fn write_preference(
        &mut self,
        key: PreferenceKey,
        value: &Value,
    ) -> Result<(), StorageError> {
        let encoded = serde_json::to_string(value).map_err(|_| StorageError::InvalidState)?;
        if encoded.len() > 65536 {
            return Err(StorageError::InvalidState);
        }
        self.connection.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key.as_str(),encoded]).map_err(|_|StorageError::Unavailable)?;
        Ok(())
    }
}
