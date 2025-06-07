use crate::actor_ref::ActorRef;
use crate::client_actor::net::MCCodec;
use crate::client_actor::stream_actor::StreamActor;
use crate::server_actor::actor::{Server, ServerMessage};
use pumpkin_protocol::client::status::{CPingResponse, CStatusResponse};
use pumpkin_protocol::server::status::{SStatusPingRequest, SStatusRequest};
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_util::codec::Framed;
use tokio_util::task::TaskTracker;

pub struct StatusHandler;

impl StatusHandler {
    pub async fn spawn(
        id: usize,
        client_address: SocketAddr,
        framer: Framed<TcpStream, MCCodec>,
        tracker: &TaskTracker,
        server: Server,
    ) {
        if let Ok(status_receiver) = server.ask(ServerMessage::GetStatus).await {
            let ping_actor = StatusActor {
                id,
                client_address,
                framer,
                status: Some(status_receiver),
            };
            tracker.spawn(ping_actor.run());
        } else {
            log::info!(
                "Server is not responsive, not spawning StatusActor {id} for {client_address:?}"
            );
        }
    }
}

impl Debug for StatusActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusActor")
            .field("id", &self.id)
            // .field("client_address", &self.client_address)
            .finish()
    }
}

struct StatusActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<TcpStream, MCCodec>,
    status: Option<oneshot::Receiver<String>>,
}

impl StreamActor<Framed<TcpStream, MCCodec>> for StatusActor {
    fn get_stream(&mut self) -> &mut Framed<TcpStream, MCCodec> {
        &mut self.framer
    }
}

impl StatusActor {
    async fn run(mut self) {
        log::debug!("{self:?} initialized");

        if self.read_empty::<SStatusRequest>().await.is_some() {
            self.handle_status_request().await;

            if let Some(ping_request) = self.read::<SStatusPingRequest>().await {
                self.handle_ping_request(ping_request).await;
            }
        }

        self.shutdown().await
    }

    async fn handle_status_request(&mut self) -> bool {
        log::trace!("{self:?} received status request");
        let status = self.status.take().expect("status receiver already taken");
        match &status.await {
            Ok(status) => {
                self.send(CStatusResponse::new(status)).await;
                true
            }
            Err(e) => {
                log::error!("{self:?} failed to get status : {e:?}");
                false
            }
        }
    }

    fn handle_ping_request(&mut self, ping: SStatusPingRequest) -> impl Future<Output = bool> {
        log::trace!("{self:?} received ping request");
        self.send(CPingResponse::new(ping.payload))
    }
}
