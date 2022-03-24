use crate::devserver::DevServerMsg;
use crate::engine::EngineBroker;
use hotwatch::blocking::Flow;
use hotwatch::{blocking::Hotwatch, Event};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
enum DebounceMsg {
    Trigger,
}

pub fn start_watching(broker: EngineBroker, debounce_wait: Duration) -> Result<(), anyhow::Error> {
    let (debounce_tx, debounce_rx) = crossbeam_channel::unbounded();

    thread::spawn(move || {
        let mut hotwatch = Hotwatch::new_with_custom_delay(Duration::from_secs(0))
            .expect("hotwatch failed to initialize!");

        hotwatch
            .watch("test/src", move |_: Event| {
                let debounce_tx = debounce_tx.clone();
                debounce_tx.send(DebounceMsg::Trigger);
                Flow::Continue
            })
            .expect("failed to watch file!");

        hotwatch.run();
    });

    thread::spawn(move || loop {
        if debounce_rx.recv().is_err() {
            panic!("internal error in fswatcher thread");
        }
        loop {
            if let Err(_) = debounce_rx.recv_timeout(debounce_wait) {
                broker.send_devserver_msg_sync(DevServerMsg::ReloadPage);
                break;
            }
        }
    });

    Ok(())
}
