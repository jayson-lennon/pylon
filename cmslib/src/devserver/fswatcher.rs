use hotwatch::blocking::Flow;
use hotwatch::{blocking::Hotwatch, Event};
use std::path::Path;
use std::thread;
use std::time::Duration;
use tracing::{instrument, trace};

use crate::devserver::broker::{EngineMsg, FilesystemUpdateEvents};

use super::EngineBroker;

#[derive(Debug)]
enum WatchMsg {
    Ev(Event),
}

#[instrument(skip(broker))]
pub fn start_watching<P: AsRef<Path> + std::fmt::Debug>(
    dirs: &[P],
    broker: EngineBroker,
    debounce_wait: Duration,
) -> Result<(), anyhow::Error> {
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
                .watch(dir, move |ev: Event| {
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
                add_event(&mut events, ev);
            }
            Err(e) => panic!("internal error in fswatcher thread: {:?}", e),
        }
        loop {
            match engine_relay_rx.recv_timeout(debounce_wait) {
                Ok(msg) => {
                    let WatchMsg::Ev(ev) = msg;
                    add_event(&mut events, ev);
                }
                Err(_) => {
                    trace!(events = ?events, "sending filesystem update events");
                    broker
                        .send_engine_msg_sync(EngineMsg::FilesystemUpdate(events))
                        .expect("error communicating with engine from filesystem watcher");
                    break;
                }
            }
        }
    });

    Ok(())
}

fn add_event(events: &mut FilesystemUpdateEvents, ev: Event) {
    use Event::*;

    match ev {
        Create(path) => events.create(path),
        Remove(path) => events.delete(path),
        Write(path) => events.change(path),
        Rename(src, dst) => {
            events.delete(src);
            events.create(dst);
        }
        _ => (),
    }
}
