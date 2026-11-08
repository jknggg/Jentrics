#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if let Some(code) =
        jentrics_core::claude_bridge::runtime::capture_cli(&std::env::args_os().collect::<Vec<_>>())
    {
        std::process::exit(code);
    }
    jentrics_core::desktop::run();
}
