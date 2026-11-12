//! `file`: the system tells which paths were touched; once a file has been still for `debounce`
//! it becomes a `created`, `changed` or `removed` event. Files already there are not new.

use super::{Live, Place};
use cluaiz_automation::trigger::files::{Settle, Stamp, Watch};
use cluaiz_automation::trigger::Spec;
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};

/// How often touched files are looked at again while some wait to be still
const LOOK_MS: u64 = 250;

fn stat(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((meta.len(), meta.modified().ok()))
}

/// The files there now that the trigger counts
fn existing(watch: &Watch) -> Vec<(PathBuf, Stamp)> {
    let mut out = Vec::new();
    let mut folders = vec![watch.root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if watch.recursive {
                    folders.push(path);
                }
            } else if watch.wants(&path) {
                if let Some(stamp) = stat(&path) {
                    out.push((path, stamp));
                }
            }
        }
    }
    out
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub async fn run(at: Place, spec: Spec) {
    let home = dirs::home_dir().unwrap_or_default();
    let watch = match Watch::of(&spec, &home) {
        Ok(watch) => watch,
        Err(message) => return at.set(|l| *l = Live::error(message)),
    };
    if !watch.root.is_dir() {
        return at.set(|l| *l = Live::error(format!("the folder {} does not exist", watch.root.display())));
    }
    let (send, mut touched) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            event.paths.into_iter().for_each(|p| {
                let _ = send.send(p);
            });
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => return at.set(|l| *l = Live::error(format!("cannot watch {}: {error}", watch.root.display()))),
    };
    let mode = if watch.recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
    if let Err(error) = watcher.watch(&watch.root, mode) {
        return at.set(|l| *l = Live::error(format!("cannot watch {}: {error}", watch.root.display())));
    }
    let mut settle = Settle::new(existing(&watch));
    loop {
        tokio::select! {
            path = touched.recv() => match path {
                Some(path) if watch.wants(&path) => settle.touch(path, now_ms()),
                Some(_) => continue,
                None => return at.set(|l| *l = Live::error("the folder watcher stopped")),
            },
            _ = tokio::time::sleep(std::time::Duration::from_millis(LOOK_MS)), if settle.waiting() => {}
        }
        for (path, event, stamp) in settle.due(now_ms(), watch.debounce_ms, stat) {
            if watch.events.iter().any(|e| e == event) {
                at.fire(Watch::data(&path, event, stamp)).await;
            }
        }
    }
}
