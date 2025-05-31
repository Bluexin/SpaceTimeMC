use crate::client_actor::net::MCCodec;
use crate::client_actor::status::StatusHandler;
use crate::server_actor::actor::Server;
use bytes::Buf;
use ferrumc_net_codec::net_types::NetTypesError;
use futures::StreamExt;
use pumpkin_data::packet::CURRENT_MC_PROTOCOL;
use pumpkin_data::packet::serverbound::HANDSHAKE_INTENTION;
use pumpkin_protocol::ser::ReadingError;
use pumpkin_protocol::ser::packet::Packet;
use pumpkin_protocol::server::handshake::SHandShake;
use pumpkin_protocol::{ConnectionState, RawPacket, ServerPacket};
use pumpkin_util::text::TextComponent;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_util::codec::Framed;
use tokio_util::task::TaskTracker;

pub struct HandshakeHandler {
    kill: oneshot::Sender<String>,
}

impl HandshakeHandler {
    pub fn spawn(
        tcp_stream: TcpStream,
        client_address: SocketAddr,
        id: usize,
        tracker: &TaskTracker,
        server: Server,
    ) -> Self {
        let (sender, receiver) = oneshot::channel();
        tcp_stream
            .set_nodelay(true)
            .expect("Failed to set TCP_NODELAY");
        let framer = Framed::new(tcp_stream, MCCodec::new());

        let actor = HandshakeActor {
            id,
            client_address,
            framer,
            server,
            kill: receiver,
            tracker: tracker.clone(),
        };
        tracker.spawn(actor.run());

        Self { kill: sender }
    }
}

struct HandshakeActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<TcpStream, MCCodec>,
    server: Server,
    kill: oneshot::Receiver<String>,
    tracker: TaskTracker,
}

impl Debug for HandshakeActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandshakeActor")
            .field("id", &self.id)
            .field("client_address", &self.client_address)
            .finish()
    }
}

impl HandshakeActor {
    async fn run(mut self) {
        log::debug!("{:?} initialized", self);

        let sel = select! {
            biased;
            /*_ = &mut self.kill => {
                log::info!("Client {} killed", self.id);
                None
            },*/
            _ = sleep(std::time::Duration::from_secs(5)) => {
                log::debug!("{:?} timeout reached", self);
                None
            },
            some = self.framer.next() => some,
        };

        let next_state = match sel {
            Some(Ok(packet)) => self.handle_packet(packet),
            Some(Err(e)) => {
                log::error!("{:?} stream error : {:?}", self, e);
                None
            }
            None => {
                log::debug!("{:?} stream closed", self);
                None
            }
        };

        match next_state {
            Some(state) => {
                log::debug!("{:?} transitioning to state {:?}", self, state);
                match state {
                    ConnectionState::HandShake => {
                        log::error!("{:?} can not transition to handshake", self);
                        self.terminate().await
                    }
                    ConnectionState::Status => {
                        StatusHandler::spawn(
                            self.id,
                            self.client_address,
                            self.framer,
                            self.kill,
                            &self.tracker,
                            self.server,
                        )
                        .await
                    }
                    ConnectionState::Login => {
                        log::info!("{:?} login not yet implemented", self);
                        self.terminate().await
                    }
                    _ => {
                        log::error!("{:?} unhandled next state {:?}", self, state);
                        self.terminate().await
                    }
                }
            }
            None => self.terminate().await,
        }
    }

    async fn terminate(mut self) {
        // Ignore possible close error as it might already be closed by this point
        let _ = self.framer.get_mut().shutdown().await;
        log::debug!("{:?} terminated", self);
    }

    fn handle_packet(&self, packet: RawPacket) -> Option<ConnectionState> {
        log::trace!(
            "{:?} received packet : {} (size {})",
            self,
            packet.id,
            packet.payload.len()
        );

        match packet.id {
            SHandShake::PACKET_ID => match SHandShake::read(packet.payload.reader()) {
                Ok(handshake) => self.handle_handshake(handshake),
                Err(e) => {
                    log::error!(
                        "{:?} failed to read packet id {} : {:?}",
                        self,
                        packet.id,
                        e
                    );
                    None
                }
            },
            _ => {
                log::error!("{:?} encountered unknown packet {}", self, packet.id);
                None
            }
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
            let protocol = version;
            match protocol.cmp(&(CURRENT_MC_PROTOCOL as i32)) {
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
}
