use std::sync::Arc;
use std::{collections::HashSet, path::PathBuf};

use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use crate::CanonicalPath;
use tokio::runtime::Handle;

use crate::core::engine::RenderedPage;

type EngineSender = async_channel::Sender<EngineMsg>;
type EngineReceiver = async_channel::Receiver<EngineMsg>;

#[derive(Debug)]
pub struct RenderPageRequest {
    tx: async_channel::Sender<Option<RenderedPage>>,
    pub canonical_path: CanonicalPath,
}

impl RenderPageRequest {
    pub fn new(
        canonical_path: CanonicalPath,
    ) -> (Self, async_channel::Receiver<Option<RenderedPage>>) {
        let (tx, rx) = async_channel::bounded(1);
        (Self { tx, canonical_path }, rx)
    }

    pub async fn send(&self, page: Option<RenderedPage>) -> Result<(), anyhow::Error> {
        Ok(self.tx.send(page).await?)
    }
    pub fn send_sync(
        &self,
        handle: Handle,
        page: Option<RenderedPage>,
    ) -> Result<(), anyhow::Error> {
        handle.block_on(async { Ok(self.tx.send(page).await?) })
    }
}

#[derive(Debug)]
pub struct FilesystemUpdateEvents {
    changed: HashSet<PathBuf>,
    deleted: HashSet<PathBuf>,
    added: HashSet<PathBuf>,
}

impl FilesystemUpdateEvents {
    #[must_use]
    pub fn new() -> Self {
        Self {
            changed: HashSet::new(),
            deleted: HashSet::new(),
            added: HashSet::new(),
        }
    }

    pub fn changed<P: Into<PathBuf>>(&mut self, path: P) {
        self.changed.insert(path.into());
    }

    pub fn deleted<P: Into<PathBuf>>(&mut self, path: P) {
        self.deleted.insert(path.into());
    }

    pub fn added<P: Into<PathBuf>>(&mut self, path: P) {
        self.added.insert(path.into());
    }

    pub fn get_changed(&self) -> impl Iterator<Item = &PathBuf> {
        self.changed.iter()
    }

    pub fn get_deleted(&self) -> impl Iterator<Item = &PathBuf> {
        self.deleted.iter()
    }

    pub fn get_added(&self) -> impl Iterator<Item = &PathBuf> {
        self.added.iter()
    }
}

#[derive(Debug)]
pub enum EngineMsg {
    FilesystemUpdate(FilesystemUpdateEvents),
    RenderPage(RenderPageRequest),
    /// Builds the site using existing configuration and source material
    Build,
    /// Rescans all source files and templates, reloads the user config,
    /// then builds the site
    Rebuild,
    /// Reloads user configuration
    ReloadUserConfig,
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

    pub fn recv_engine_msg_sync(&self) -> Result<EngineMsg, anyhow::Error> {
        self.rt
            .handle()
            .block_on(async { self.recv_engine_msg().await })
    }
}
