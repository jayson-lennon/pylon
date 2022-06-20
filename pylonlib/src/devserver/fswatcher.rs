use hotwatch::blocking::Flow;
use hotwatch::blocking::Hotwatch;
use std::collections::HashSet;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;

use crate::devserver::broker::EngineMsg;
use crate::USER_LOG;
use crate::{AbsPath, Result};

use super::EngineBroker;

#[derive(Debug)]
pub struct FilesystemUpdateEvents {
    changed: HashSet<AbsPath>,
    deleted: HashSet<AbsPath>,
    created: HashSet<AbsPath>,
}

impl FilesystemUpdateEvents {
    pub fn new() -> Self {
        Self {
            changed: HashSet::new(),
            deleted: HashSet::new(),
            created: HashSet::new(),
        }
    }

    pub fn change(&mut self, path: &AbsPath) {
        self.changed.insert(path.clone());
    }

    pub fn delete(&mut self, path: &AbsPath) {
        self.deleted.insert(path.clone());
    }

    pub fn create(&mut self, path: &AbsPath) {
        self.created.insert(path.clone());
    }

    pub fn changed(&self) -> impl Iterator<Item = &AbsPath> {
        self.changed.iter()
    }

    pub fn deleted(&self) -> impl Iterator<Item = &AbsPath> {
        self.deleted.iter()
    }

    pub fn created(&self) -> impl Iterator<Item = &AbsPath> {
        self.created.iter()
    }

    pub fn len(&self) -> usize {
        self.changed.len() + self.deleted.len() + self.created.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for FilesystemUpdateEvents {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum WatchMsg {
    Ev(hotwatch::Event),
}

pub fn start_watching<P: AsRef<Path> + std::fmt::Debug>(
    dirs: &[P],
    broker: EngineBroker,
    debounce_wait: Duration,
) -> Result<()> {
    trace!("start watching directories");

    let dirs = dirs
        .iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect::<Vec<_>>();

    let (engine_relay_tx, engine_relay_rx) = crossbeam_channel::unbounded();

    trace!("spawning watcher thread");
    thread::spawn(move || {
        let mut hotwatch = Hotwatch::new_with_custom_delay(Duration::from_secs(0))
            .expect("hotwatch failed to initialize!");

        for dir in &dirs {
            let engine_relay_tx = engine_relay_tx.clone();
            hotwatch
                .watch(dir, move |ev: hotwatch::Event| {
                    engine_relay_tx
                        .send(WatchMsg::Ev(ev))
                        .expect("error communicating with debounce thread on filesystem watcher");
                    Flow::Continue
                })
                .expect("failed to watch file!");
        }

        hotwatch.run();
    });

    trace!("spawning engine comms thread");
    thread::spawn(move || loop {
        let mut events = FilesystemUpdateEvents::new();
        match engine_relay_rx.recv() {
            Ok(msg) => {
                let WatchMsg::Ev(ev) = msg;
                if let Err(e) = add_event(&mut events, ev) {
                    error!("failed to create fswatch event: {}", e);
                }
            }
            Err(e) => panic!("internal error in fswatcher thread: {:?}", e),
        }
        loop {
            if let Ok(msg) = engine_relay_rx.recv_timeout(debounce_wait) {
                let WatchMsg::Ev(ev) = msg;
                if let Err(e) = add_event(&mut events, ev) {
                    error!("failed to add fswatch event: {}", e);
                }
            } else {
                info!(
                    target: USER_LOG,
                    "{} filesystem events detected",
                    events.len()
                );
                broker
                    .send_engine_msg_sync(EngineMsg::FilesystemUpdate(events))
                    .expect("error communicating with engine from filesystem watcher");
                break;
            }
        }
    });

    Ok(())
}

fn add_event(events: &mut FilesystemUpdateEvents, ev: hotwatch::Event) -> Result<()> {
    use hotwatch::Event::*;

    match ev {
        Create(path) => {
            debug!(target: USER_LOG, "file created: {}", path.display());
            events.create(&AbsPath::new(path)?);
        }
        Remove(path) => {
            debug!(target: USER_LOG, "file deleted: {}", path.display());
            events.delete(&AbsPath::new(path)?);
        }
        Write(path) => {
            debug!(target: USER_LOG, "file updated: {}", path.display());
            events.change(&AbsPath::new(path)?);
        }
        Rename(src, dst) => {
            debug!(
                target: USER_LOG,
                "file renamed: {} -> {}",
                src.display(),
                dst.display()
            );
            events.delete(&AbsPath::new(src)?);
            events.create(&AbsPath::new(dst)?);
        }
        _ => (),
    }
    Ok(())
}
