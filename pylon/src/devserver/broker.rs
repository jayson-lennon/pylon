use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::core::engine::{Engine, EnginePaths};
use crate::core::page::RenderedPage;
use crate::core::pagestore::SearchKey;
use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use crate::{RelPath, Result};
use tokio::runtime::Handle;
use tracing::{error, trace, warn};

use super::fswatcher::FilesystemUpdateEvents;

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

    pub fn inner(&self) -> &ToEngine {
        &self.inner
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
    RenderPage(EngineRequest<SearchKey, Result<Option<RenderedPage>>>),
    ProcessMounts(EngineRequest<(), Result<()>>),
    ProcessPipelines(EngineRequest<RelPath, Result<()>>),
    /// Quits the application
    Quit,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RenderBehavior {
    Memory,
    Write,
}

impl std::str::FromStr for RenderBehavior {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match s.as_ref() {
            "memory" => Ok(RenderBehavior::Memory),
            "write" => Ok(RenderBehavior::Write),
            _ => Err("unknown render behavior".to_owned()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineBroker {
    rt: Arc<tokio::runtime::Runtime>,
    engine_paths: Arc<EnginePaths>,
    devserver: (DevServerSender, DevServerReceiver),
    engine: (EngineSender, EngineReceiver),
    render_behavior: RenderBehavior,
}

impl EngineBroker {
    pub fn new(
        rt: Arc<tokio::runtime::Runtime>,
        behavior: RenderBehavior,
        engine_paths: Arc<EnginePaths>,
    ) -> Self {
        Self {
            rt,
            engine_paths,
            devserver: async_channel::unbounded(),
            engine: async_channel::unbounded(),
            render_behavior: behavior,
        }
    }

    pub fn handle(&self) -> Handle {
        self.rt.handle().clone()
    }

    pub fn engine_paths(&self) -> Arc<EnginePaths> {
        self.engine_paths.clone()
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
        paths: Arc<EnginePaths>,
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
            let mut engine = Engine::new(paths)?;

            // engine.process_mounts(engine.rules().mounts())?;

            let _devserver = engine.start_devserver(bind, debounce_ms, broker.clone())?;

            loop {
                if let Err(e) = handle_msg::process_mounts(&engine) {
                    warn!(err=%e, "failed processing mounts")
                }
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
                                handle_msg::render(&engine, chan.inner(), broker.render_behavior)
                            });
                        }

                        EngineMsg::FilesystemUpdate(events) => {
                            let _ws_msg = broker.send_devserver_msg_sync(DevServerMsg::Notify(
                                "Building Assets...".to_owned(),
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
    use std::ffi::OsStr;

    use anyhow::Context;
    use tracing::{error, instrument, trace};

    use crate::{
        core::{
            engine::{Engine, PipelineBehavior},
            page::RenderedPage,
            Page,
        },
        devserver::broker::RenderBehavior,
        AbsPath, CheckedFilePath, RelPath, Result, SysPath,
    };

    use super::FilesystemUpdateEvents;

    pub fn process_pipelines(engine: &Engine, page_path: &RelPath) -> Result<()> {
        let abs_path = AbsPath::new(engine.paths().output_dir())?.join(page_path);

        let raw_html = std::fs::read_to_string(&abs_path)?;

        let sys_path = SysPath::from_abs_path(
            &abs_path,
            engine.paths().project_root(),
            engine.paths().output_dir(),
        )?;

        let html_path = CheckedFilePath::new(&sys_path)?;
        let html_assets = crate::discover::html_asset::find(engine.paths(), &html_path, &raw_html)?;

        // check that each required asset was processed
        {
            let unhandled_assets =
                engine.run_pipelines(&html_assets, PipelineBehavior::Overwrite)?;
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
    pub fn render<S: AsRef<str> + std::fmt::Debug>(
        engine: &Engine,
        search_key: S,
        render_behavior: RenderBehavior,
    ) -> Result<Option<RenderedPage>> {
        trace!(search_key = ?search_key, "receive render page message");

        if let Some(page) = engine.page_store().get(&search_key.as_ref().into()) {
            let lints = engine
                .lint(std::iter::once(page))
                .with_context(|| "failed to lint page")?;
            if lints.has_deny() {
                Err(anyhow::anyhow!(lints.to_string()))
            } else {
                let rendered = engine
                    .render(std::iter::once(page))
                    .with_context(|| "failed to render page")?
                    .into_iter()
                    .next()
                    .unwrap();

                if render_behavior == RenderBehavior::Write {
                    let parent_dir = rendered.target().without_file_name().to_absolute_path();
                    crate::util::make_parent_dirs(&parent_dir)?;
                    std::fs::write(rendered.target().to_absolute_path(), &rendered.html())?;
                }

                // asset discovery & pipeline processing
                {
                    let html_path = CheckedFilePath::new(&page.target())?;
                    let mut html_assets = crate::discover::html_asset::find(
                        engine.paths(),
                        &html_path,
                        &rendered.html(),
                    )
                    .with_context(|| "failed to discover HTML assets")?;
                    html_assets.drop_offsite();

                    let unhandled_assets = engine
                        .run_pipelines(&html_assets, PipelineBehavior::Overwrite)
                        .with_context(|| "failed to run pipelines")?;
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
        for path in events.changed() {
            let relative_path = {
                let engine_paths = engine.paths();
                let project_base = engine_paths.project_root();
                path.to_relative(&project_base)?
            };

            // reload any updated pages
            if relative_path.starts_with(engine.paths().src_dir())
                && relative_path.extension() == Some(OsStr::new("md"))
            {
                let checked_path = {
                    let rel = relative_path.strip_prefix(engine.paths().src_dir())?;
                    let sys_path = SysPath::new(
                        engine.paths().project_root(),
                        engine.paths().src_dir(),
                        &rel,
                    );
                    CheckedFilePath::new(&sys_path)?
                };
                let page = Page::from_file(engine.paths(), checked_path, engine.renderers())?;
                // update will automatically insert the page if it doesn't exist
                let _ = engine.page_store_mut().update(page);
            }

            // reload templates
            if relative_path.starts_with(&engine.paths().template_dir()) {
                reload_templates = true;
            }

            // reload rules
            if path == &engine.paths().absolute_rule_script() {
                reload_rules = true;
            }
        }

        if reload_rules {
            engine.reload_rules()?;
        }

        if reload_templates {
            engine.reload_template_engines()?;
        }

        Ok(())
    }
}
