use crate::client_actor::mc_socket;
use crate::client_actor::net::MCCodec;
use crate::client_actor::stream_actor::StreamActor;
use crate::server_actor::actor::Server;
use pumpkin::net::GameProfile;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::task::TaskTracker;

pub struct ConfigurationHandler;

impl ConfigurationHandler {
    pub async fn spawn(
        id: usize,
        client_address: SocketAddr,
        framer: Framed<MCSocket, MCCodec>,
        tracker: TaskTracker,
        server: Server,
        profile: GameProfile,
    ) {
        let config_actor = ConfigurationActor {
            id,
            client_address,
            framer,
            server,
            tracker: tracker.clone(),
            profile,
        };
        tracker.spawn(config_actor.run());
    }
}

impl Debug for ConfigurationActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigurationActor")
            .field("id", &self.id)
            // .field("client_address", &self.client_address)
            .finish()
    }
}

type MCSocket = mc_socket::MCSocket<TcpStream>;

struct ConfigurationActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<MCSocket, MCCodec>,
    server: Server,
    tracker: TaskTracker,
    profile: GameProfile,
}

impl StreamActor<Framed<MCSocket, MCCodec>> for ConfigurationActor {
    fn get_stream(&mut self) -> &mut Framed<MCSocket, MCCodec> {
        &mut self.framer
    }
}

impl ConfigurationActor {
    async fn run(mut self) {
        log::debug!("{self:?} initialized");

        self.shutdown().await
    }
}
