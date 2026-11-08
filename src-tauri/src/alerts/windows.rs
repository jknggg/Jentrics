use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowsNotificationState {
    Accepting,
    Busy,
    NotPresent,
    FullScreen,
    Presentation,
    QuietTime,
    StoreApp,
    Unavailable,
}
#[cfg(windows)]
pub fn windows_state() -> WindowsNotificationState {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state = 0;
    // Read-only interruption status; this does not inspect or change per-app
    // notification permissions, Do Not Disturb, or presentation settings.
    if unsafe { SHQueryUserNotificationState(&mut state) } < 0 {
        return WindowsNotificationState::Unavailable;
    }
    match state {
        1 => WindowsNotificationState::NotPresent,
        2 => WindowsNotificationState::Busy,
        3 => WindowsNotificationState::FullScreen,
        4 => WindowsNotificationState::Presentation,
        5 => WindowsNotificationState::Accepting,
        6 => WindowsNotificationState::QuietTime,
        7 => WindowsNotificationState::StoreApp,
        _ => WindowsNotificationState::Unavailable,
    }
}
#[cfg(not(windows))]
pub fn windows_state() -> WindowsNotificationState {
    WindowsNotificationState::Unavailable
}
