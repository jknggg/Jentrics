use super::AppState;
use notify::{RecursiveMode, Watcher};
use std::{path::PathBuf, sync::Arc};
impl AppState {
    pub async fn watch_claude_capture(self: Arc<Self>, directory: PathBuf) {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(event)
                    if event.paths.iter().any(|path| {
                        path.file_name()
                            .is_some_and(|v| v == crate::claude_bridge::ingress::CAPTURE_FILE)
                    }) =>
                {
                    match sender.try_send(()) {
                        Ok(()) | Err(tokio::sync::mpsc::error::TrySendError::Full(())) => (),
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => {
                            eprintln!("Jentrics capture watcher stopped.")
                        }
                    }
                }
                Ok(_) => (),
                Err(error) => eprintln!("Jentrics capture watcher failed: {error}"),
            });
        let mut watcher = match watcher {
            Ok(v) => v,
            Err(error) => {
                eprintln!("Jentrics capture watcher unavailable: {error}");
                return;
            }
        };
        if let Err(error) = watcher.watch(&directory, RecursiveMode::NonRecursive) {
            eprintln!("Jentrics capture directory watch failed: {error}");
            return;
        }
        let mut shutdown = self.shutdown.subscribe();
        loop {
            if *shutdown.borrow() {
                return;
            }
            tokio::select! {biased;_=shutdown.changed()=>return,event=receiver.recv()=>{if event.is_none(){eprintln!("Jentrics capture watcher disconnected.");return;}self.refresh_claude_capture();}}
        }
    }
}
