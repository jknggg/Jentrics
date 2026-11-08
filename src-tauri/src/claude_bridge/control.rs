use super::{
    ingress,
    setup::{self, SetupError, SetupPlan},
    MAX_CAPTURE_BYTES,
};
use serde::Serialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeIssue {
    ExistingStatusLine,
    HelperUnavailable,
    SettingsUnavailable,
    UnsafePath,
    ChangedSinceReview,
    InvalidSettings,
    CaptureUnavailable,
    BackupConflict,
    ConfirmationRequired,
    NotConfigured,
    ExpiredReview,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub configured: Option<bool>,
    pub can_setup: bool,
    pub can_restore: bool,
    pub settings_path: Option<String>,
    pub capture_path: Option<String>,
    pub command: Option<String>,
    pub last_capture_ms: Option<i64>,
    pub issue: Option<BridgeIssue>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeReview {
    pub confirmation_id: String,
    pub settings_path: String,
    pub capture_path: String,
    pub command: String,
}
struct Pending {
    id: String,
    created_ms: i64,
    plan: SetupPlan,
}
pub struct BridgeControl {
    settings: PathBuf,
    capture: PathBuf,
    helper: PathBuf,
    pending: Mutex<Option<Pending>>,
}
impl BridgeControl {
    pub fn new(settings: PathBuf, capture: PathBuf, helper: PathBuf) -> Self {
        Self {
            settings,
            capture,
            helper,
            pending: Mutex::new(None),
        }
    }
    pub fn capture_dir(&self) -> &Path {
        &self.capture
    }
    pub fn status(&self, now_ms: i64) -> BridgeStatus {
        let mut status = BridgeStatus {
            configured: None,
            can_setup: false,
            can_restore: false,
            settings_path: self.settings.to_str().map(str::to_owned),
            capture_path: self.capture.to_str().map(str::to_owned),
            command: None,
            last_capture_ms: None,
            issue: None,
        };
        let result = (|| {
            if !self.helper.is_file() {
                return Err(BridgeIssue::HelperUnavailable);
            }
            let empty = setup::plan_setup(None, &self.helper, &self.capture).map_err(issue)?;
            status.command = Some(empty.command.clone());
            let bytes = read_settings(&self.settings)?;
            let fields = bytes
                .as_ref()
                .map(|v| {
                    serde_json::from_slice::<serde_json::Value>(v)
                        .map_err(|_| BridgeIssue::InvalidSettings)
                })
                .transpose()?;
            if fields.as_ref().is_some_and(|v| !v.is_object()) {
                return Err(BridgeIssue::InvalidSettings);
            }
            status.can_restore = setup::backup_path(&self.settings).is_file();
            let current = fields.as_ref().and_then(|v| v.get("statusLine"));
            status.configured = Some(
                current.and_then(|v| v.get("type")).and_then(|v| v.as_str()) == Some("command")
                    && current
                        .and_then(|v| v.get("command"))
                        .and_then(|v| v.as_str())
                        == Some(empty.command.as_str()),
            );
            if current.is_some() && status.configured != Some(true) {
                return Err(BridgeIssue::ExistingStatusLine);
            }
            if status.configured != Some(true) && status.can_restore {
                return Err(BridgeIssue::BackupConflict);
            }
            status.can_setup = status.configured == Some(false);
            status.last_capture_ms = ingress::read_capture(&self.capture, now_ms)
                .map_err(|_| BridgeIssue::CaptureUnavailable)?
                .map(|v| v.observed_ms);
            Ok(())
        })();
        status.issue = result.err();
        status
    }
    pub fn prepare(&self, now_ms: i64) -> Result<BridgeReview, BridgeIssue> {
        let mut pending = self.pending.lock().expect("Claude setup lock poisoned");
        *pending = None;
        if !self.helper.is_file() {
            return Err(BridgeIssue::HelperUnavailable);
        }
        if setup::backup_path(&self.settings).exists() {
            return Err(BridgeIssue::BackupConflict);
        }
        let bytes = read_settings(&self.settings)?;
        let plan =
            setup::plan_setup(bytes.as_deref(), &self.helper, &self.capture).map_err(issue)?;
        let id = format!("claude-{}-{}", std::process::id(), now_ms);
        let review = BridgeReview {
            confirmation_id: id.clone(),
            settings_path: self
                .settings
                .to_str()
                .ok_or(BridgeIssue::UnsafePath)?
                .into(),
            capture_path: self.capture.to_str().ok_or(BridgeIssue::UnsafePath)?.into(),
            command: plan.command.clone(),
        };
        *pending = Some(Pending {
            id,
            created_ms: now_ms,
            plan,
        });
        Ok(review)
    }
    pub fn apply(
        &self,
        id: &str,
        confirmed: bool,
        now_ms: i64,
    ) -> Result<BridgeStatus, BridgeIssue> {
        let pending = self
            .pending
            .lock()
            .expect("Claude setup lock poisoned")
            .take()
            .ok_or(BridgeIssue::ChangedSinceReview)?;
        if !confirmed {
            return Err(BridgeIssue::ConfirmationRequired);
        }
        if id != pending.id {
            return Err(BridgeIssue::ChangedSinceReview);
        }
        if now_ms < pending.created_ms || now_ms - pending.created_ms > 120_000 {
            return Err(BridgeIssue::ExpiredReview);
        }
        if let Some(parent) = self.settings.parent() {
            std::fs::create_dir_all(parent).map_err(|_| BridgeIssue::SettingsUnavailable)?;
        }
        setup::apply_setup(&pending.plan, &self.settings, true).map_err(issue)?;
        Ok(self.status(now_ms))
    }
    pub fn restore(&self, confirmed: bool, now_ms: i64) -> Result<BridgeStatus, BridgeIssue> {
        if !confirmed {
            return Err(BridgeIssue::ConfirmationRequired);
        }
        *self.pending.lock().expect("Claude setup lock poisoned") = None;
        setup::restore_setup(&self.settings, true).map_err(issue)?;
        Ok(self.status(now_ms))
    }
}
fn read_settings(path: &Path) -> Result<Option<Vec<u8>>, BridgeIssue> {
    let file = match std::fs::File::open(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(BridgeIssue::SettingsUnavailable),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BridgeIssue::SettingsUnavailable)?;
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err(BridgeIssue::InvalidSettings);
    }
    Ok(Some(bytes))
}
fn issue(error: SetupError) -> BridgeIssue {
    match error {
        SetupError::ExistingStatusLine => BridgeIssue::ExistingStatusLine,
        SetupError::UnsafePath => BridgeIssue::UnsafePath,
        SetupError::NotConfirmed => BridgeIssue::ConfirmationRequired,
        SetupError::ChangedSinceReview => BridgeIssue::ChangedSinceReview,
        SetupError::BackupExists => BridgeIssue::BackupConflict,
        SetupError::Malformed | SetupError::TooLarge => BridgeIssue::InvalidSettings,
        SetupError::Io => BridgeIssue::SettingsUnavailable,
    }
}
