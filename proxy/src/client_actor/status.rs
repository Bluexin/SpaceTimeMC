use crate::client_actor::net::{BytesSerializable, MCCodec};
use crate::server_actor::actor::{Server, ServerMessage};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use ferrumc_net_codec::net_types::NetTypesError;
use futures::{SinkExt, StreamExt};
use pumpkin_data::packet::CURRENT_MC_PROTOCOL;
use pumpkin_data::packet::serverbound::{STATUS_PING_REQUEST, STATUS_STATUS_REQUEST};
use pumpkin_protocol::client::status::{CPingResponse, CStatusResponse};
use pumpkin_protocol::codec::var_int::VarIntType;
use pumpkin_protocol::ser::packet::Packet;
use pumpkin_protocol::ser::{ReadingError, WritingError};
use pumpkin_protocol::server::handshake::SHandShake;
use pumpkin_protocol::server::status::{SStatusPingRequest, SStatusRequest};
use pumpkin_protocol::{ClientPacket, ConnectionState, RawPacket, ServerPacket};
use pumpkin_util::text::TextComponent;
use serde::Serialize;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;
use tokio::time::sleep;
use tokio_util::codec::Framed;
use tokio_util::task::TaskTracker;

pub struct StatusHandler;

impl StatusHandler {
    pub async fn spawn(
        id: usize,
        client_address: SocketAddr,
        framer: Framed<TcpStream, MCCodec>,
        kill: oneshot::Receiver<String>,
        tracker: &TaskTracker,
        server: Server,
    ) {
        let (status_sender, status_receiver) = oneshot::channel();
        server
            .send(ServerMessage::GetStatus {
                reply_to: status_sender,
            })
            .await;

        let ping_actor = StatusActor {
            id,
            client_address,
            framer,
            kill,
            status: Some(status_receiver),
        };
        tracker.spawn(ping_actor.run());
    }
}

impl Debug for StatusActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusActor")
            .field("id", &self.id)
            .field("client_address", &self.client_address)
            .finish()
    }
}

struct StatusActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<TcpStream, MCCodec>,
    // server: Server,
    kill: oneshot::Receiver<String>,
    status: Option<oneshot::Receiver<String>>,
}

impl StatusActor {
    async fn run(mut self) {
        log::debug!("{:?} initialized", self);

        while match select! {
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
        } {
            Some(Ok(packet)) => self.handle_packet(packet).await,
            Some(Err(e)) => {
                log::error!("{:?} stream error : {:?}", self, e);
                false
            }
            None => {
                log::debug!("{:?} stream closed", self);
                false
            }
        } {}

        // Ignore possible close error as it might already be closed by this point
        let _ = self.framer.get_mut().shutdown().await;
        log::debug!("{:?} terminated", self);
    }

    async fn handle_packet(&mut self, packet: RawPacket) -> bool {
        log::trace!(
            "{:?} received packet : {} (size {})",
            self,
            packet.id,
            packet.payload.len()
        );

        match packet.id {
            SStatusRequest::PACKET_ID => self.handle_status_request().await,
            SStatusPingRequest::PACKET_ID => {
                match SStatusPingRequest::read(packet.payload.reader()) {
                    Ok(ping) => self.handle_ping_request(ping).await,
                    Err(e) => {
                        log::error!("{:?} failed to read ping request : {:?}", self, e);
                        false
                    }
                }
            }
            _ => {
                log::error!("{:?} failed to handle packet id {}", self, packet.id);
                false
            }
        }
    }

    async fn handle_status_request(&mut self) -> bool {
        log::trace!("{:?} received status request", self);
        let status = self.status.take().expect("status receiver already taken");
        match &status.await {
            Ok(status) => {
                self.send(CStatusResponse::new(&status)).await;
                true
            }
            Err(e) => {
                log::error!("{:?} failed to get status : {:?}", self, e);
                false
            }
        }
    }

    async fn handle_ping_request(&mut self, ping: SStatusPingRequest) -> bool {
        log::trace!("{:?} received ping request", self);
        self.send(CPingResponse::new(ping.payload)).await
    }

    /// This is split from send because status response currently includes ref to self via status_response_json
    async fn send_bytes(
        &mut self,
        maybe_bytes: Result<Bytes, WritingError>,
        packet_id: VarIntType,
    ) -> bool {
        match maybe_bytes {
            Ok(bytes) => {
                let _ = self.framer.send(bytes).await;
                true
            }
            Err(e) => {
                log::error!("{:?} failed to write packet {} : {:?}", self, packet_id, e);
                false
            }
        }
    }

    async fn send<P: ClientPacket>(&mut self, p: P) -> bool {
        self.send_bytes(p.serialize_bytes(), P::PACKET_ID).await
    }
}
