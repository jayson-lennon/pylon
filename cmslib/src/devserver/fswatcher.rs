use crate::core::broker::{EngineBroker, EngineMsg};
use hotwatch::blocking::Flow;
use hotwatch::{blocking::Hotwatch, Event};
use std::path::Path;
use std::thread;
use std::time::Duration;
use tracing::{instrument, trace};

#[derive(Debug)]
enum DebounceMsg {
    Trigger,
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

    let (debounce_tx, debounce_rx) = crossbeam_channel::unbounded();

    trace!("spawning watcher thread");
    thread::spawn(move || {
        let mut hotwatch = Hotwatch::new_with_custom_delay(Duration::from_secs(0))
            .expect("hotwatch failed to initialize!");

        for dir in dirs.iter() {
            let debounce_tx = debounce_tx.clone();
            hotwatch
                .watch(dir, move |_: Event| {
                    debounce_tx
                        .send(DebounceMsg::Trigger)
                        .expect("error communicating with debounce thread on filesystem watcher");
                    Flow::Continue
                })
                .expect("failed to watch file!");
        }

        hotwatch.run();
    });

    trace!("spawning debouncer thread");
    thread::spawn(move || loop {
        if debounce_rx.recv().is_err() {
            panic!("internal error in fswatcher thread");
        }
        loop {
            if let Err(_) = debounce_rx.recv_timeout(debounce_wait) {
                broker
                    .send_engine_msg_sync(EngineMsg::Rebuild)
                    .expect("error communicating with engine from filesystem watcher");
                break;
            }
        }
    });

    Ok(())
}
