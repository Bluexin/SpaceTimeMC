use crate::client_actor::login::LoginHandler;
use crate::client_actor::net::MCCodec;
use crate::client_actor::status::StatusHandler;
use crate::client_actor::stream_actor::StreamActor;
use crate::server_actor::actor::Server;
use bytes::Buf;
use pumpkin_protocol::ser::packet::Packet;
use pumpkin_protocol::server::handshake::SHandShake;
use pumpkin_protocol::{ConnectionState, RawPacket, ServerPacket};
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::task::TaskTracker;

pub struct HandshakeHandler;

impl HandshakeHandler {
    pub fn spawn(
        tcp_stream: TcpStream,
        client_address: SocketAddr,
        id: usize,
        tracker: &TaskTracker,
        server: Server,
    ) -> Self {
        tcp_stream
            .set_nodelay(true)
            .expect("Failed to set TCP_NODELAY");

        let framer = Framed::new(tcp_stream, Default::default());

        let actor = HandshakeActor {
            id,
            client_address,
            framer,
            server,
            tracker: tracker.clone(),
        };
        tracker.spawn(actor.run());

        Self
    }
}

struct HandshakeActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<TcpStream, MCCodec>,
    server: Server,
    tracker: TaskTracker,
}

impl Debug for HandshakeActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandshakeActor")
            .field("id", &self.id)
            // .field("client_address", &self.client_address)
            .finish()
    }
}

impl StreamActor<Framed<TcpStream, MCCodec>> for HandshakeActor {
    fn get_stream(&mut self) -> &mut Framed<TcpStream, MCCodec> {
        &mut self.framer
    }
}

impl HandshakeActor {
    async fn run(mut self) {
        // TODO : maybe not have client ip in the logs at all ?
        log::debug!("{self:?} initialized for {}", self.client_address);

        let next_state = self
            .read::<SHandShake>()
            .await
            .and_then(|packet| self.handle_handshake(packet));

        match next_state {
            Some(state) => {
                log::debug!("{self:?} transitioning to state {state:?}");
                match state {
                    ConnectionState::Status => self.transition_status().await,
                    ConnectionState::Login => self.transition_login().await,
                    _ => {
                        log::error!("{self:?} can not transition to {state:?}");
                        self.shutdown().await
                    }
                }
            }
            None => self.shutdown().await,
        }
    }

    fn handle_handshake(&self, handshake: SHandShake) -> Option<ConnectionState> {
        let version = handshake.protocol_version.0;
        // self.protocol_version
        //     .store(version, std::sync::atomic::Ordering::Relaxed);
        let server_address = handshake.server_address;
        // *self.server_address.lock().await = handshake.server_address;

        log::debug!(
            "{:?} Handshake received : version {}, address {}, port {}, next_state {:?}",
            self,
            version,
            server_address,
            handshake.server_port,
            handshake.next_state
        );
        // TODO : check version compat etc
        Some(handshake.next_state)
        // self.connection_state.store(handshake.next_state);
        /*if self.connection_state.load() != ConnectionState::Status {
            let macros = version;
            match macros.cmp(&(CURRENT_MC_PROTOCOL as i32)) {
                std::cmp::Ordering::Less => {
                    self.kick(TextComponent::translate(
                        "multiplayer.disconnect.outdated_client",
                        [TextComponent::text(CURRENT_MC_VERSION.to_string())],
                    ))
                        .await;
                }
                std::cmp::Ordering::Equal => {}
                std::cmp::Ordering::Greater => {
                    self.kick(TextComponent::translate(
                        "multiplayer.disconnect.incompatible",
                        [TextComponent::text(CURRENT_MC_VERSION.to_string())],
                    ))
                        .await;
                }
            }
        }*/
    }

    async fn transition_status(self) {
        StatusHandler::spawn(
            self.id,
            self.client_address,
            self.framer,
            &self.tracker,
            self.server,
        )
        .await
    }

    async fn transition_login(self) {
        LoginHandler::spawn(
            self.id,
            self.client_address,
            self.framer,
            self.tracker,
            self.server,
        )
        .await
    }
}
