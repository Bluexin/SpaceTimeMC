use bytes::{Buf, BufMut, Bytes, BytesMut};
use ferrumc_net_codec::net_types::NetTypesError;
use ferrumc_net_codec::net_types::var_int::VarInt;
use log::logger;
use pumpkin_protocol::ser::WritingError;
use pumpkin_protocol::{ClientPacket, RawPacket};
use std::io::Cursor;
use tokio_util::codec::{Decoder, Encoder};

// TODO : output straight up deserialized packets ?
// But if the connection is not in the right state, we should not attempt to deser an invalid (id-wise) packet
pub struct MCCodec {
    state: DecodeState,
}

enum DecodeState {
    Len,
    Data(usize),
}

impl MCCodec {
    pub fn new() -> Self {
        MCCodec {
            state: DecodeState::Len,
        }
    }

    pub fn set_compression(&mut self, compression: u8) {
        todo!("Not yet implemented")
    }

    fn decode_head(&self, src: &mut BytesMut) -> Result<Option<usize>, NetTypesError> {
        if src.len() < 1 {
            // No need to even try in this case
            return Ok(None);
        }

        let n = {
            let mut c = Cursor::new(&*src);
            match VarInt::read(&mut c) {
                Ok(n) => n,
                // This happens when we get an early end of buffer
                Err(NetTypesError::Io(_)) => return Ok(None),
                // FIXME : own error types
                Err(e) => return Err(e),
            }
        };

        src.advance(n.len);
        let len = n.val as usize;
        // Ensure that the buffer has enough space to read the incoming payload
        src.reserve(len);
        Ok(Some(len))
    }

    fn decode_body(
        &self,
        n: usize,
        src: &mut BytesMut,
    ) -> Result<Option<RawPacket>, NetTypesError> {
        if src.len() < n {
            log::info!("Not enough data to decode packet ({} < {})", src.len(), n);
            Ok(None)
        } else {
            // let mut current_packet = src.split_to(n);
            let id = VarInt::read(&mut src.reader())?;
            Ok(Some(RawPacket {
                payload: src.split_to(n - id.len).into(),
                id: id.val,
            }))
        }
    }
}

impl Decoder for MCCodec {
    type Item = RawPacket;
    type Error = NetTypesError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // TODO : check against max packet size
        let n = match self.state {
            DecodeState::Len => match self.decode_head(src)? {
                Some(n) => {
                    log::trace!("Decoded packet length : {}", n);
                    self.state = DecodeState::Data(n);
                    n
                }
                None => return Ok(None),
            },
            DecodeState::Data(n) => n,
        };

        match self.decode_body(n, src) {
            Ok(None) => Ok(None),
            Ok(Some(data)) => {
                log::trace!(
                    "Decoded packet : {} (size {}) {:?}",
                    data.id,
                    data.payload.len(),
                    data.payload
                );
                self.state = DecodeState::Len;
                // Make sure the buffer has enough space to read the next len
                src.reserve(1);
                Ok(Some(data))
            }
            Err(e) => {
                log::error!("Error decoding packet : {:?}", e);
                Err(e)
            }
        }
    }
}

impl Encoder<Bytes> for MCCodec {
    type Error = NetTypesError;

    fn encode(&mut self, data: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let n = data.len();
        // TODO : check if there is a max server packet length
        let var_length = VarInt::from(n);
        dst.reserve(var_length.len + n);

        var_length.write(&mut dst.writer())?;
        dst.extend_from_slice(&data[..]);

        Ok(())
    }
}

pub trait BytesSerializable {
    fn serialize_bytes(&self) -> Result<Bytes, WritingError>;
}

impl<P: ClientPacket> BytesSerializable for P {
    fn serialize_bytes(&self) -> Result<Bytes, WritingError> {
        let mut bytes = BytesMut::new().writer();
        self.write(&mut bytes)?;
        Ok(bytes.into_inner().into())
    }
}
