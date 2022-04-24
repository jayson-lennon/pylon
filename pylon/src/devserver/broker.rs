use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::{collections::HashSet, path::PathBuf};

use crate::core::config::EngineConfig;
use crate::core::engine::Engine;
use crate::core::page::RenderedPage;
use crate::core::{RelSystemPath, Uri};
use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use crate::Result;
use tokio::runtime::Handle;
use tracing::{error, trace, warn};

type EngineSender = async_channel::Sender<EngineMsg>;
type EngineReceiver = async_channel::Receiver<EngineMsg>;

#[derive(Debug)]
pub struct EngineRequest<ToEngine, FromEngine>
where
    ToEngine: Send + Sync + 'static,
    FromEngine: Send + Sync + 'static,
{
    tx: async_channel::Sender<FromEngine>,
    inner: ToEngine,
}

impl<ToEngine, FromEngine> EngineRequest<ToEngine, FromEngine>
where
    ToEngine: Send + Sync + 'static,
    FromEngine: Send + Sync + 'static,
{
    pub fn new(data: ToEngine) -> (Self, async_channel::Receiver<FromEngine>) {
        let (tx, rx) = async_channel::bounded(1);
        (Self { tx, inner: data }, rx)
    }

    pub async fn respond(&self, data: FromEngine) -> Result<()> {
        Ok(self.tx.send(data).await?)
    }
    pub fn respond_sync(&self, handle: &Handle, data: FromEngine) -> Result<()> {
        handle.block_on(async { Ok(self.tx.send(data).await?) })
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
    RenderPage(EngineRequest<Uri, Result<Option<RenderedPage>>>),
    ProcessMounts(EngineRequest<(), Result<()>>),
    ProcessPipelines(EngineRequest<String, Result<()>>),
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

    pub async fn send_devserver_msg(&self, msg: DevServerMsg) -> Result<()> {
        Ok(self.devserver.0.send(msg).await?)
    }

    pub async fn send_engine_msg(&self, msg: EngineMsg) -> Result<()> {
        Ok(self.engine.0.send(msg).await?)
    }

    pub fn send_engine_msg_sync(&self, msg: EngineMsg) -> Result<()> {
        self.rt
            .handle()
            .block_on(async { self.send_engine_msg(msg).await })
    }

    pub fn send_devserver_msg_sync(&self, msg: DevServerMsg) -> Result<()> {
        self.rt
            .handle()
            .block_on(async { self.send_devserver_msg(msg).await })
    }

    pub async fn recv_devserver_msg(&self) -> Result<DevServerMsg> {
        Ok(self.devserver.1.recv().await?)
    }

    pub fn recv_devserver_msg_sync(&self) -> Result<DevServerMsg> {
        self.rt
            .handle()
            .block_on(async { self.recv_devserver_msg().await })
    }

    async fn recv_engine_msg(&self) -> Result<EngineMsg> {
        Ok(self.engine.1.recv().await?)
    }

    fn recv_engine_msg_sync(&self) -> Result<EngineMsg> {
        self.rt
            .handle()
            .block_on(async { self.recv_engine_msg().await })
    }

    pub fn spawn_engine_thread<S: Into<SocketAddr> + std::fmt::Debug>(
        &self,
        config: EngineConfig,
        bind: S,
        debounce_ms: u64,
    ) -> Result<JoinHandle<Result<()>>> {
        macro_rules! respond_sync {
            ($chan:ident, $handle:expr, $fn:block) => {
                if let Err(e) = $chan.respond_sync($handle, $fn) {
                    warn!(err = %e, "tried to respond on a closed channel");
                }
            };
        }
        trace!("spawning engine thread");

        let bind = bind.into();
        let broker = self.clone();
        let engine_handle = thread::spawn(move || {
            let mut engine = Engine::new(config)?;

            // engine.process_mounts(engine.rules().mounts())?;

            let _devserver = engine.start_devserver(bind, debounce_ms, broker.clone())?;

            loop {
                match broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::ProcessMounts(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::process_mounts(&engine)
                            });
                        }
                        EngineMsg::ProcessPipelines(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::process_pipelines(&engine, &chan.inner)
                            });
                        }
                        EngineMsg::RenderPage(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::render(&engine, &chan.inner)
                            });
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
    use std::path::PathBuf;

    use tracing::{error, instrument, trace};

    use crate::{
        core::{engine::Engine, page::RenderedPage, Page, RelSystemPath, Uri},
        Result,
    };

    use super::FilesystemUpdateEvents;

    pub fn process_pipelines<S: AsRef<str>>(engine: &Engine, page_path: S) -> Result<()> {
        let sys_path = RelSystemPath::new(
            &engine.config().target_root,
            &PathBuf::from(page_path.as_ref()),
        );
        let raw_html = std::fs::read_to_string(sys_path.to_path_buf())?;
        let html_assets = crate::discover::html_asset::find(&sys_path, &raw_html)?;

        // check that each required asset was processed
        {
            let unhandled_assets = engine.run_pipelines(&html_assets)?;
            for asset in &unhandled_assets {
                error!(asset = ?asset, "missing asset");
            }
            if !unhandled_assets.is_empty() {
                return Err(anyhow::anyhow!("one or more assets are missing"));
            }
        }
        Ok(())
    }

    pub fn process_mounts(engine: &Engine) -> Result<()> {
        engine.process_mounts(engine.rules().mounts())
    }

    #[instrument(skip_all)]
    pub fn render(engine: &Engine, uri: &Uri) -> Result<Option<RenderedPage>> {
        trace!(uri = ?uri, "receive render page message");

        engine.process_mounts(engine.rules().mounts())?;

        if let Some(page) = engine.page_store().get(uri) {
            let lints = engine.lint(std::iter::once(page))?;
            if lints.has_deny() {
                Err(anyhow::anyhow!(lints.to_string()))
            } else {
                let rendered = engine
                    .render(std::iter::once(page))?
                    .into_iter()
                    .next()
                    .unwrap();

                // asset discovery & pipeline processing
                {
                    let html_assets =
                        crate::discover::html_asset::find(&rendered.target, &rendered.html)?;
                    let unhandled_assets = engine.run_pipelines(&html_assets)?;
                    // check for missing assets in pages
                    {
                        for asset in &unhandled_assets {
                            error!(asset = ?asset, "missing asset");
                        }
                        if !unhandled_assets.is_empty() {
                            return Err(anyhow::anyhow!("one or more assets are missing"));
                        }
                    }
                }
                Ok(Some(rendered))
            }
        } else {
            Ok(None)
        }
    }

    #[instrument(skip_all)]
    pub fn fs_event(engine: &mut Engine, events: FilesystemUpdateEvents) -> Result<()> {
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
