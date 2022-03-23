use crate::devserver::{DevServerEvent, DevServerSender};
use hotwatch::blocking::Flow;
use hotwatch::{blocking::Hotwatch, Event};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Debug)]
enum DebounceMsg {
    Trigger,
}

pub fn start_watching(
    rt: Arc<Runtime>,
    live_reload_tx: DevServerSender,
    debounce_duration: Duration,
) -> Result<(), anyhow::Error> {
    let (debounce_tx, debounce_rx) = async_channel::unbounded();

    let rt_debounce = Arc::clone(&rt);
    thread::spawn(move || {
        rt_debounce.block_on(async move {
            use tokio::time::sleep;
            loop {
                let _ = debounce_rx.recv().await;
                loop {
                    tokio::select! {
                        _ = sleep(debounce_duration) => break,
                        _ = debounce_rx.recv() => {dbg!("hello");},
                    }
                }
                match live_reload_tx.send(DevServerEvent::ReloadPage).await {
                    Ok(_) => {
                        dbg!("sent message to reload page dev server");
                    }
                    Err(_) => {
                        dbg!("failed sending reload page event to dev server");
                    }
                }
            }
        });
    });

    thread::spawn(move || {
        let mut hotwatch = Hotwatch::new_with_custom_delay(Duration::from_secs(0))
            .expect("hotwatch failed to initialize!");

        hotwatch
            .watch("test/src", move |_: Event| {
                let debounce_tx = debounce_tx.clone();
                rt.spawn(async move {
                    let _ = debounce_tx.send(DebounceMsg::Trigger).await;
                });
                Flow::Continue
            })
            .expect("failed to watch file!");

        hotwatch.run();
    });

    Ok(())
}
