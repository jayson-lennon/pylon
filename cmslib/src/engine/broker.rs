use crate::devserver::{DevServerMsg, DevServerReceiver, DevServerSender};
use tokio::runtime::Handle;

type EngineSender = async_channel::Sender<EngineMsg>;
type EngineReceiver = async_channel::Receiver<EngineMsg>;

#[derive(Debug, Clone)]
pub enum EngineMsg {
    /// Builds the site using existing configuration and source material
    Build,
    /// Builds the site using existing configuration, but rescans all source material
    Rebuild,
    /// Starts the development server
    StartDevServer(std::net::SocketAddr, u64),
    /// Reloads user configuration
    ReloadUserConfig,
    /// Quits the application
    Quit,
}

#[derive(Debug, Clone)]
pub struct EngineBroker {
    rt_handle: Handle,
    devserver: (DevServerSender, DevServerReceiver),
    engine: (EngineSender, EngineReceiver),
}

impl EngineBroker {
    pub fn new(handle: Handle) -> Self {
        Self {
            rt_handle: handle,
            devserver: async_channel::unbounded(),
            engine: async_channel::unbounded(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.rt_handle.clone()
    }

    pub async fn send_devserver_msg(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        Ok(self.devserver.0.send(msg).await?)
    }

    pub async fn send_engine_msg(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        Ok(self.engine.0.send(msg).await?)
    }

    pub fn send_engine_msg_sync(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        self.rt_handle
            .block_on(async { self.send_engine_msg(msg).await })
    }

    pub fn send_devserver_msg_sync(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        self.rt_handle
            .block_on(async { self.send_devserver_msg(msg).await })
    }

    pub async fn recv_devserver_msg(&self) -> Result<DevServerMsg, anyhow::Error> {
        Ok(self.devserver.1.recv().await?)
    }

    pub fn recv_devserver_msg_sync(&self) -> Result<DevServerMsg, anyhow::Error> {
        self.rt_handle
            .block_on(async { self.recv_devserver_msg().await })
    }

    async fn recv_engine_msg(&self) -> Result<EngineMsg, anyhow::Error> {
        println!("try to recv engine msg async");
        Ok(self.engine.1.recv().await?)
    }

    pub fn recv_engine_msg_sync(&self) -> Result<EngineMsg, anyhow::Error> {
        println!("try to recv engine msg");
        self.rt_handle
            .block_on(async { self.recv_engine_msg().await })
    }
}
