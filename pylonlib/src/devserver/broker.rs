use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::core::engine::{Engine, GlobalEnginePaths};
use crate::core::library::SearchKey;
use crate::core::page::RenderedPage;
use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use crate::Result;

use tokio::runtime::Handle;
use tracing::{error, trace, warn};
use typed_uri::Uri;

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
    ProcessPipelines(EngineRequest<Uri, Result<()>>),
    ProcessMounts(EngineRequest<(), Result<()>>),
    /// Quits the application
    Quit,

    Ping,
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
    engine_paths: GlobalEnginePaths,
    devserver: (DevServerSender, DevServerReceiver),
    engine: (EngineSender, EngineReceiver),
    render_behavior: RenderBehavior,
}

impl EngineBroker {
    pub fn new(
        rt: Arc<tokio::runtime::Runtime>,
        behavior: RenderBehavior,
        engine_paths: GlobalEnginePaths,
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

    pub fn engine_paths(&self) -> GlobalEnginePaths {
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
        paths: GlobalEnginePaths,
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
                match broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::ProcessMounts(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::mount_directories(&engine)
                            });
                        }
                        EngineMsg::RenderPage(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::render_page(
                                    &engine,
                                    chan.inner(),
                                    broker.render_behavior,
                                )
                            });
                        }
                        EngineMsg::ProcessPipelines(chan) => {
                            respond_sync!(chan, &broker.handle(), {
                                handle_msg::process_pipelines(&engine, &chan.inner)
                            });
                        }

                        EngineMsg::FilesystemUpdate(events) => {
                            let _ws_msg = broker.send_devserver_msg_sync(DevServerMsg::Notify(
                                "Building Assets...".to_owned(),
                            ));

                            if let Err(e) = handle_msg::fs_event(&mut engine, &events) {
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
                        EngineMsg::Ping => {}
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
    use std::{collections::HashSet, ffi::OsStr};

    use eyre::{eyre, WrapErr};

    use tracing::trace;
    use typed_uri::Uri;

    use crate::{
        core::{
            engine::{step, Engine},
            page::RenderedPage,
            Page,
        },
        devserver::broker::RenderBehavior,
        discover::html_asset::{HtmlAssets, MissingAssetsError},
        Result, SysPath,
    };

    use super::FilesystemUpdateEvents;

    pub fn mount_directories(engine: &Engine) -> Result<()> {
        step::mount_directories(engine.rules().mounts())
    }

    pub fn process_pipelines(engine: &Engine, uri: &Uri) -> Result<()> {
        let sys_path = uri
            .to_sys_path(engine.paths().project_root(), engine.paths().output_dir())
            .wrap_err_with(|| format!("Failed to generate SysPath from {}", uri))?;

        let html_path = sys_path.confirm(pathmarker::HtmlFile)?;

        let missing_assets = step::build_required_asset_list(engine, std::iter::once(&html_path))
            .map(|mut assets| {
                assets.drop_offsite();
                assets
            })
            .wrap_err_with(|| format!("Failed find assets in file '{}'", &html_path))?
            .into_iter()
            .filter(step::filter::not_on_disk)
            .collect::<HtmlAssets>();

        let missing_assets = step::run_pipelines(engine, &missing_assets)
            .wrap_err("Failed to run pipelines in dev server")?
            .into_iter()
            .filter(|asset| step::filter::not_on_disk(*asset))
            .collect::<HashSet<_>>();

        if !missing_assets.is_empty() {
            return Err(eyre!(missing_assets
                .iter()
                .map(|asset| asset.uri().unconfirmed())
                .collect::<MissingAssetsError>()));
        }
        Ok(())
    }

    pub fn render_page<S: AsRef<str> + std::fmt::Debug>(
        engine: &Engine,
        search_key: S,
        render_behavior: RenderBehavior,
    ) -> Result<Option<RenderedPage>> {
        trace!(search_key = ?search_key, "receive render page message");

        if let Some(page) = engine.library().get(&search_key.as_ref().into()) {
            let lints = step::run_lints(engine, std::iter::once(page))
                .wrap_err_with(|| format!("Failed to run lints for page '{}'", page.uri()))?;
            let _cli_report = step::report::lints(&lints);
            if lints.has_deny() {
                Err(eyre!(lints.to_string()))
            } else {
                let rendered_collection = step::render(engine, std::iter::once(page))
                    .wrap_err_with(|| format!("Failed to render page '{}'", page.uri()))?;

                if render_behavior == RenderBehavior::Write {
                    rendered_collection.write_to_disk().wrap_err(
                        "Failed to write rendered page to disk with RenderBehavior::Write",
                    )?;
                }

                let rendered_page = rendered_collection.into_iter().next().unwrap();
                let html_path = page.target().confirm(pathmarker::HtmlFile)?;

                let missing_assets =
                    step::build_required_asset_list(engine, std::iter::once(&html_path))
                        .wrap_err("Failed to discover HTML assets during single page render")
                        .map(|mut assets| {
                            assets.drop_offsite();
                            assets
                        })?;
                let missing_assets = step::run_pipelines(engine, &missing_assets)
                    .wrap_err("Failed to run pipelines during single page render")?
                    .into_iter()
                    .filter(|asset| step::filter::not_on_disk(*asset))
                    .collect::<HashSet<_>>();

                if !missing_assets.is_empty() {
                    return Err(eyre!(missing_assets
                        .iter()
                        .map(|asset| asset.uri().unconfirmed())
                        .collect::<MissingAssetsError>()));
                }

                Ok(Some(rendered_page))
            }
        } else {
            Ok(None)
        }
    }

    pub fn fs_event(engine: &mut Engine, events: &FilesystemUpdateEvents) -> Result<()> {
        trace!(events = ?events, "receive file system update message");
        let mut reload_templates = false;
        let mut reload_rules = false;
        for path in events.changed() {
            let relative_path = {
                let engine_paths = engine.paths();
                let project_base = engine_paths.project_root();
                path.to_relative(project_base)?
            };

            // reload any updated pages
            if relative_path.starts_with(engine.paths().content_dir())
                && relative_path.extension() == Some(OsStr::new("md"))
            {
                let checked_path = {
                    let rel = relative_path.strip_prefix(engine.paths().content_dir())?;
                    SysPath::new(
                        engine.paths().project_root(),
                        engine.paths().content_dir(),
                        &rel,
                    )
                    .confirm(pathmarker::MdFile)
                    .wrap_err_with(|| {
                        format!("Failed to confirm path on fsevent for path '{}'", path)
                    })?
                };
                let page = Page::from_file(engine.paths(), checked_path, engine.renderers())
                    .wrap_err_with(|| {
                        format!(
                            "Failed to create new page from filesystem event at '{}'",
                            path
                        )
                    })?;
                // update will automatically insert the page if it doesn't exist
                let _ = engine.library_mut().update(page);
            }

            // reload templates
            if relative_path.starts_with(&engine.paths().template_dir()) {
                reload_templates = true;
            }

            // reload rules
            if path == &engine.paths().abs_rule_script() {
                reload_rules = true;
            }
        }

        if reload_rules {
            engine
                .reload_rules()
                .wrap_err("Failed to reload rules during fs event")?;
        }

        if reload_templates {
            engine
                .reload_template_engines()
                .wrap_err("Failed to reload template engines during fs event")?;
        }

        Ok(())
    }
}
