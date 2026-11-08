// Setup is opt-in. The caller must bind confirmation to the exact reviewed plan.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const EXISTING_STATUS_LINE_INSTRUCTIONS: &str = "Your existing Claude status line was preserved. Automatic forwarding is unavailable for this shell. Keep your command unchanged; manually integrate the Jentrics capture helper only after verifying that your wrapper forwards the same stdin to both commands and preserves your original command's stdout and exit status. Jentrics does not read transcripts or replace your command.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupError {
    Malformed,
    TooLarge,
    ExistingStatusLine,
    UnsafePath,
    NotConfirmed,
    ChangedSinceReview,
    BackupExists,
    Io,
}

pub struct SetupPlan {
    pub command: String,
    original: Option<Vec<u8>>,
    proposed: Vec<u8>,
}

/// Pure preview: never reads or writes user configuration. Keep this plan in the
/// backend; send only its command/review summary to the UI, never raw settings.
pub fn plan_setup(
    settings: Option<&[u8]>,
    helper: &Path,
    directory: &Path,
) -> Result<SetupPlan, SetupError> {
    let mut config = match settings {
        Some(bytes) => parse_settings(bytes)?,
        None => json!({}),
    };
    let fields = config.as_object_mut().ok_or(SetupError::Malformed)?;
    if fields.contains_key("statusLine") {
        return Err(SetupError::ExistingStatusLine);
    }
    let helper = quoted_path(helper)?;
    let directory = quoted_path(directory)?;
    // Explicit PowerShell invocation is supported through either Claude Windows
    // shell. Paths reject characters that Git Bash could expand inside quotes.
    let command = format!("powershell -NoProfile -NonInteractive -Command \"& '{helper}' --capture-dir '{directory}' | Out-Default; exit (Get-Variable LASTEXITCODE -ValueOnly)\"");
    fields.insert(
        "statusLine".into(),
        json!({"type":"command", "command":command}),
    );
    let proposed = serde_json::to_vec_pretty(&config).map_err(|_| SetupError::Malformed)?;
    if proposed.len() > super::MAX_CAPTURE_BYTES {
        return Err(SetupError::TooLarge);
    }
    Ok(SetupPlan {
        command,
        original: settings.map(Vec::from),
        proposed,
    })
}

/// `confirmed` must be supplied only after the user reviews this exact plan.
/// Backup intentionally includes the original settings; never emit it via IPC.
pub fn apply_setup(
    plan: &SetupPlan,
    settings_path: &Path,
    confirmed: bool,
) -> Result<(), SetupError> {
    if !confirmed {
        return Err(SetupError::NotConfirmed);
    }
    if read_optional(settings_path, super::MAX_CAPTURE_BYTES)? != plan.original {
        return Err(SetupError::ChangedSinceReview);
    }
    let backup = Backup {
        schema_version: 1,
        original: plan
            .original
            .as_ref()
            .map(|bytes| String::from_utf8(bytes.clone()))
            .transpose()
            .map_err(|_| SetupError::Malformed)?,
        installed: String::from_utf8(plan.proposed.clone()).map_err(|_| SetupError::Malformed)?,
    };
    let backup_path = backup_path(settings_path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(backup_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                SetupError::BackupExists
            } else {
                SetupError::Io
            }
        })?;
    let bytes = serde_json::to_vec(&backup).map_err(|_| SetupError::Malformed)?;
    file.write_all(&bytes).map_err(|_| SetupError::Io)?;
    file.sync_all().map_err(|_| SetupError::Io)?;
    atomic_write(settings_path, &plan.proposed)
}

pub fn restore_setup(settings_path: &Path, confirmed: bool) -> Result<(), SetupError> {
    if !confirmed {
        return Err(SetupError::NotConfirmed);
    }
    let backup_path = backup_path(settings_path);
    let bytes =
        read_optional(&backup_path, super::MAX_CAPTURE_BYTES * 16)?.ok_or(SetupError::Io)?;
    let backup: Backup = serde_json::from_slice(&bytes).map_err(|_| SetupError::Malformed)?;
    if backup.schema_version != 1 {
        return Err(SetupError::Malformed);
    }
    parse_settings(backup.installed.as_bytes())?;
    if let Some(original) = &backup.original {
        parse_settings(original.as_bytes())?;
    }
    let current = read_optional(settings_path, super::MAX_CAPTURE_BYTES)?;
    let original = backup
        .original
        .as_ref()
        .map(|text| text.as_bytes().to_vec());
    // Also allow recovery when installation failed after the durable backup.
    if current.as_deref() != Some(backup.installed.as_bytes()) && current != original {
        return Err(SetupError::ChangedSinceReview);
    }
    match original {
        Some(bytes) => atomic_write(settings_path, &bytes)?,
        None if current.is_some() => {
            std::fs::remove_file(settings_path).map_err(|_| SetupError::Io)?
        }
        None => (),
    }
    std::fs::remove_file(backup_path).map_err(|_| SetupError::Io)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Backup {
    schema_version: u8,
    original: Option<String>,
    installed: String,
}

fn parse_settings(bytes: &[u8]) -> Result<Value, SetupError> {
    if bytes.len() > super::MAX_CAPTURE_BYTES {
        return Err(SetupError::TooLarge);
    }
    let config: Value = serde_json::from_slice(bytes).map_err(|_| SetupError::Malformed)?;
    if !config.is_object() {
        return Err(SetupError::Malformed);
    }
    Ok(config)
}

fn quoted_path(path: &Path) -> Result<String, SetupError> {
    let text = path
        .to_str()
        .ok_or(SetupError::UnsafePath)?
        .replace('\\', "/");
    if !path.is_absolute() || text.chars().any(|c| c.is_control() || "'\"$`!".contains(c)) {
        return Err(SetupError::UnsafePath);
    }
    Ok(text)
}

pub fn backup_path(settings_path: &Path) -> PathBuf {
    let mut name = settings_path.as_os_str().to_owned();
    name.push(".jentrics-backup.json");
    PathBuf::from(name)
}

fn read_optional(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, SetupError> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(SetupError::Io),
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupError::Io)?;
    if bytes.len() > limit {
        return Err(SetupError::TooLarge);
    }
    Ok(Some(bytes))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let directory = path.parent().ok_or(SetupError::UnsafePath)?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|_| SetupError::Io)?;
    file.write_all(bytes).map_err(|_| SetupError::Io)?;
    file.as_file().sync_all().map_err(|_| SetupError::Io)?;
    file.persist(path).map_err(|_| SetupError::Io)?;
    Ok(())
}
