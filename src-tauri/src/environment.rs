use std::{ffi::OsString, fmt, path::PathBuf, time::SystemTime};

#[derive(Debug, PartialEq, Eq)]
pub enum EnvironmentError {
    SearchPathUnavailable,
    InvalidSystemTime,
}

impl fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SearchPathUnavailable => "PATH must contain an absolute search directory",
            Self::InvalidSystemTime => "System clock is outside the supported UTC range",
        })
    }
}

impl std::error::Error for EnvironmentError {}

pub fn search_dirs() -> Result<Vec<PathBuf>, EnvironmentError> {
    search_dirs_from(std::env::var_os("PATH"))
}

pub fn search_dirs_from(value: Option<OsString>) -> Result<Vec<PathBuf>, EnvironmentError> {
    let value = value.ok_or(EnvironmentError::SearchPathUnavailable)?;
    let directories: Vec<_> = std::env::split_paths(&value)
        .filter(|directory| directory.is_absolute())
        .collect();
    if directories.is_empty() {
        return Err(EnvironmentError::SearchPathUnavailable);
    }
    Ok(directories)
}

pub fn timestamp_ms(time: SystemTime) -> Result<i64, EnvironmentError> {
    let elapsed = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EnvironmentError::InvalidSystemTime)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| EnvironmentError::InvalidSystemTime)
}
