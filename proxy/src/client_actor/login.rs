use crate::client_actor::mc_socket;
use crate::client_actor::net::MCCodec;
use crate::client_actor::stream_actor::StreamActor;
use crate::server_actor::actor::Server;
use pumpkin_protocol::server::login::SLoginStart;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, FramedParts};
use tokio_util::task::TaskTracker;

pub struct LoginHandler;

impl LoginHandler {
    pub async fn spawn(
        id: usize,
        client_address: SocketAddr,
        framer: Framed<TcpStream, MCCodec>,
        tracker: &TaskTracker,
        server: Server,
    ) {
        let FramedParts {
            io: stream, codec, ..
        } = framer.into_parts();
        let login_actor = LoginActor {
            id,
            client_address,
            framer: Framed::from_parts(FramedParts::new(MCSocket::new(stream), codec)),
            server,
        };
        tracker.spawn(login_actor.run());
    }
}

impl Debug for LoginActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginActor")
            .field("id", &self.id)
            .field("client_address", &self.client_address)
            .finish()
    }
}

type MCSocket = mc_socket::MCSocket<TcpStream>;

struct LoginActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<MCSocket, MCCodec>,
    server: Server,
}

impl StreamActor<Framed<MCSocket, MCCodec>> for LoginActor {
    fn get_stream(&mut self) -> &mut Framed<MCSocket, MCCodec> {
        &mut self.framer
    }
}

impl LoginActor {
    async fn run(mut self) {
        log::debug!("{self:?} initialized");

        if let Some(login) = self.read::<SLoginStart>().await {
            let (new_self, _ok) = self.handle_login_start(login).await;
            self = new_self;
        }

        self.shutdown().await
    }

    fn encrypt(mut self, key: &[u8; 16]) -> Result<Self, &str> {
        let FramedParts {
            io: stream, codec, ..
        } = self.framer.into_parts();
        let new_socket = stream.encrypt(key)?;
        self.framer = Framed::from_parts(FramedParts::new(new_socket, codec));
        Ok(self)
    }

    async fn handle_login_start(self, login: SLoginStart) -> (Self, bool) {
        log::debug!(
            "{:?} received login start : {} ({})",
            self,
            login.uuid,
            login.name
        );
        (self, false)
    }
}
