use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::{collections::HashSet, path::PathBuf};

use crate::core::config::EngineConfig;
use crate::core::engine::Engine;
use crate::core::page::RenderedPage;
use crate::core::Uri;
use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use tokio::runtime::Handle;
use tracing::{error, trace};

type EngineSender = async_channel::Sender<EngineMsg>;
type EngineReceiver = async_channel::Receiver<EngineMsg>;

type RenderPageResponse = Result<Option<RenderedPage>, anyhow::Error>;

#[derive(Debug)]
pub struct RenderPageRequest {
    tx: async_channel::Sender<RenderPageResponse>,
    pub uri: Uri,
}

impl RenderPageRequest {
    pub fn new(uri: Uri) -> (Self, async_channel::Receiver<RenderPageResponse>) {
        let (tx, rx) = async_channel::bounded(1);
        (Self { tx, uri }, rx)
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub async fn respond(&self, page: RenderPageResponse) -> Result<(), anyhow::Error> {
        Ok(self.tx.send(page).await?)
    }
    pub fn respond_sync(
        &self,
        handle: &Handle,
        page: RenderPageResponse,
    ) -> Result<(), anyhow::Error> {
        handle.block_on(async { Ok(self.tx.send(page).await?) })
    }
}

#[derive(Debug)]
pub struct FilesystemUpdateEvents {
    changed: HashSet<PathBuf>,
    deleted: HashSet<PathBuf>,
    created: HashSet<PathBuf>,
}

impl FilesystemUpdateEvents {
    #[must_use]
    pub fn new() -> Self {
        Self {
            changed: HashSet::new(),
            deleted: HashSet::new(),
            created: HashSet::new(),
        }
    }

    pub fn change<P: Into<PathBuf>>(&mut self, path: P) {
        self.changed.insert(path.into());
    }

    pub fn delete<P: Into<PathBuf>>(&mut self, path: P) {
        self.deleted.insert(path.into());
    }

    pub fn create<P: Into<PathBuf>>(&mut self, path: P) {
        self.created.insert(path.into());
    }

    pub fn changed(&self) -> impl Iterator<Item = &PathBuf> {
        self.changed.iter()
    }

    pub fn deleted(&self) -> impl Iterator<Item = &PathBuf> {
        self.deleted.iter()
    }

    pub fn created(&self) -> impl Iterator<Item = &PathBuf> {
        self.created.iter()
    }
}

impl Default for FilesystemUpdateEvents {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum EngineMsg {
    /// A group of files have been updated. This will trigger a page
    /// reload after processing is complete. Events are batched by
    /// the filesystem watcher using debouncing, so only one reload
    /// message is fired for multiple changes.
    FilesystemUpdate(FilesystemUpdateEvents),
    /// Renders a page and then returns it on the channel supplied in
    /// the request.
    RenderPage(RenderPageRequest),
    /// Quits the application
    Quit,
}

#[derive(Debug, Clone)]
pub struct EngineBroker {
    rt: Arc<tokio::runtime::Runtime>,
    devserver: (DevServerSender, DevServerReceiver),
    engine: (EngineSender, EngineReceiver),
}

impl EngineBroker {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            rt,
            devserver: async_channel::unbounded(),
            engine: async_channel::unbounded(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.rt.handle().clone()
    }

    pub async fn send_devserver_msg(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        Ok(self.devserver.0.send(msg).await?)
    }

    pub async fn send_engine_msg(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        Ok(self.engine.0.send(msg).await?)
    }

    pub fn send_engine_msg_sync(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        self.rt
            .handle()
            .block_on(async { self.send_engine_msg(msg).await })
    }

    pub fn send_devserver_msg_sync(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        self.rt
            .handle()
            .block_on(async { self.send_devserver_msg(msg).await })
    }

    pub async fn recv_devserver_msg(&self) -> Result<DevServerMsg, anyhow::Error> {
        Ok(self.devserver.1.recv().await?)
    }

    pub fn recv_devserver_msg_sync(&self) -> Result<DevServerMsg, anyhow::Error> {
        self.rt
            .handle()
            .block_on(async { self.recv_devserver_msg().await })
    }

    async fn recv_engine_msg(&self) -> Result<EngineMsg, anyhow::Error> {
        Ok(self.engine.1.recv().await?)
    }

    fn recv_engine_msg_sync(&self) -> Result<EngineMsg, anyhow::Error> {
        self.rt
            .handle()
            .block_on(async { self.recv_engine_msg().await })
    }

    pub fn spawn_engine_thread<S: Into<SocketAddr> + std::fmt::Debug>(
        &self,
        config: EngineConfig,
        bind: S,
        debounce_ms: u64,
    ) -> Result<JoinHandle<Result<(), anyhow::Error>>, anyhow::Error> {
        trace!("spawning engine thread");

        let bind = bind.into();
        let broker = self.clone();
        let engine_handle = thread::spawn(move || {
            let mut engine = Engine::new(config)?;

            let _devserver =
                engine.start_devserver(bind, debounce_ms, engine.config(), broker.clone())?;

            loop {
                match broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::RenderPage(request) => {
                            let page = handle_msg::render(&engine, &request);
                            request
                                .respond_sync(&broker.handle(), page)
                                .expect("failed to respond to render page request. this is a bug");
                        }

                        EngineMsg::FilesystemUpdate(events) => {
                            let _ws_msg = broker.send_devserver_msg_sync(DevServerMsg::Notify(
                                "buiding assets".to_owned(),
                            ));

                            if let Err(e) = handle_msg::fs_event(&mut engine, events) {
                                error!(error=%e, "fswatch error");
                                let _ws_msg = broker
                                    .send_devserver_msg_sync(DevServerMsg::Notify(e.to_string()));
                                continue;
                            }
                            // notify websocket server to reload all connected clients
                            broker.send_devserver_msg_sync(DevServerMsg::ReloadPage)?;
                        }
                        EngineMsg::Quit => {
                            break;
                        }
                    },
                    Err(e) => panic!("problem receiving from engine channel: {e}"),
                }
            }
            Ok(())
        });

        Ok(engine_handle)
    }
}

mod handle_msg {
    use tracing::{instrument, trace};

    use crate::core::{engine::Engine, page::RenderedPage, Page};

    use super::{FilesystemUpdateEvents, RenderPageRequest};

    #[instrument(skip_all)]
    pub fn render(
        engine: &Engine,
        client_request: &RenderPageRequest,
    ) -> Result<Option<RenderedPage>, anyhow::Error> {
        use crate::core::page::render::rewrite_asset_targets;

        trace!(client_request = ?client_request, "receive render page message");
        let page: Option<RenderedPage> = {
            if let Some(page) = engine.page_store().get(client_request.uri()) {
                let mut rendered = engine
                    .render(std::iter::once(page))?
                    .into_iter()
                    .next()
                    .unwrap();
                let linked_assets = rewrite_asset_targets(
                    std::slice::from_mut(&mut rendered),
                    engine.page_store(),
                )?;
                engine.run_pipelines(&linked_assets)?;
                Some(rendered)
            } else {
                None
            }
        };
        Ok(page)
    }

    #[instrument(skip_all)]
    pub fn fs_event(
        engine: &mut Engine,
        events: FilesystemUpdateEvents,
    ) -> Result<(), anyhow::Error> {
        trace!(events = ?events, "receive file system update message");
        let mut reload_templates = false;
        let mut reload_rules = false;
        for changed in events.changed() {
            // These paths come in as absolute paths. Use strip_prefix
            // to transform them into relative paths which can then
            // be used with the engine config.
            let path = {
                let cwd = std::env::current_dir()?;
                changed.strip_prefix(cwd)?
            };
            if path.starts_with(&engine.config().src_root)
                && path.extension().unwrap_or_default().to_string_lossy() == "md"
            {
                let page = Page::from_file(
                    &engine.config().src_root.as_path(),
                    &engine.config().target_root.as_path(),
                    &path,
                    engine.renderers(),
                )?;
                // update will automatically insert the page if it doesn't exist
                let _ = engine.page_store_mut().update(page);
            }

            if path.starts_with(&engine.config().template_root) {
                reload_templates = true;
            }

            if path == engine.config().rule_script {
                reload_rules = true;
            }
        }

        if reload_templates {
            engine.reload_template_engines()?;
        }

        if reload_rules {
            engine.reload_rules()?;
        }

        Ok(())
    }
}
