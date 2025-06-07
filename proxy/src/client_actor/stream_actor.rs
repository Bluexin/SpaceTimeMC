use crate::client_actor::net::BytesSerializable;
use bytes::buf::Reader;
use bytes::{Buf, Bytes};
use ferrumc_net_codec::net_types::NetTypesError;
use futures::stream::Next;
use futures::{Sink, SinkExt, Stream, StreamExt};
use pumpkin_protocol::codec::var_int::VarIntType;
use pumpkin_protocol::ser::packet::Packet;
use pumpkin_protocol::ser::{ReadingError, WritingError};
use pumpkin_protocol::{ClientPacket, RawPacket, ServerPacket};
use std::fmt::Debug;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tokio::time::sleep;
use tokio_util::codec::Framed;

pub trait StreamActor<T: Stream<Item = Result<RawPacket, NetTypesError>> + Unpin + Sink<Bytes>>
where
    Self: Debug,
{
    fn get_stream(&mut self) -> &mut T;

    fn next_frame(&mut self) -> Next<'_, T> {
        self.get_stream().next()
    }

    fn next_frame_with_timeout(&mut self) -> impl Future<Output = Option<RawPacket>> {
        async move {
            match select! {
                biased;
                /*_ = &mut self.kill => {
                    log::info!("Client {} killed", self.id);
                    None
                },*/
                _ = sleep(std::time::Duration::from_secs(5)) => {
                    log::debug!("{self:?} timeout reached");
                    None
                },
                some = self.next_frame() => some,
            } {
                Some(Ok(packet)) => Some(packet),
                Some(Err(e)) => {
                    log::error!("{self:?} stream error : {e:?}");
                    None
                }
                None => {
                    log::debug!("{self:?} stream closed");
                    None
                }
            }
        }
    }

    fn send<P: ClientPacket>(&mut self, p: P) -> impl Future<Output = bool> {
        self.send_bytes(p.serialize_bytes(), P::PACKET_ID)
    }

    fn send_bytes(
        &mut self,
        maybe_bytes: Result<Bytes, WritingError>,
        packet_id: VarIntType,
    ) -> impl Future<Output = bool> {
        async move {
            match maybe_bytes {
                Ok(bytes) => {
                    let _ = self.get_stream().send(bytes).await;
                    true
                }
                Err(e) => {
                    log::error!("{self:?} failed to write packet {packet_id} : {e:?}");
                    false
                }
            }
        }
    }

    fn read<P: Packet + ServerPacket>(&mut self) -> impl Future<Output = Option<P>> {
        self.read_custom::<_, P, P>(P::read)
    }

    fn read_empty<P: Packet>(&mut self) -> impl Future<Output = Option<()>> {
        self.read_custom::<_, P, ()>(|_| Ok(()))
    }

    fn read_custom<Read, P: Packet, Res>(&mut self, read: Read) -> impl Future<Output = Option<Res>>
    where
        Read: FnOnce(Reader<Bytes>) -> Result<Res, ReadingError>,
    {
        async move {
            if let Some(packet) = self.next_frame_with_timeout().await {
                if packet.id == P::PACKET_ID {
                    match read(packet.payload.reader()) {
                        Ok(packet) => packet.into(),
                        Err(e) => {
                            log::error!("{self:?} failed to read packet {} : {e:?}", packet.id);
                            None
                        }
                    }
                } else {
                    log::error!(
                        "{self:?} received unexpected packet : {:?} (expected: {})",
                        packet.id,
                        P::PACKET_ID
                    );
                    None
                }
            } else {
                None
            }
        }
    }

    fn shutdown(&mut self) -> impl Future<Output = ()>
    where
        T: Shutdown,
    {
        async move {
            // Ignore possible close error as it might already be closed by this point
            let _ = self.get_stream().shutdown().await;
            log::debug!("{self:?} terminated");
        }
    }
}

pub trait Shutdown {
    fn shutdown(&mut self) -> impl Future<Output = std::io::Result<()>>;
}

impl<Socket: AsyncWriteExt + Unpin, Codec> Shutdown for Framed<Socket, Codec> {
    fn shutdown(&mut self) -> impl Future<Output = std::io::Result<()>> {
        self.get_mut().shutdown()
    }
}
