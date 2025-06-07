use aes::cipher::{BlockDecryptMut, BlockSizeUser, KeyIvInit};
use aes::Aes128;
use cfb8::{Decryptor, Encryptor};
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub enum MCSocket<T> {
    Cleartext {
        stream: T,
    },
    Encrypted {
        stream: T,
        decrypt: Decryptor<Aes128>,
        encrypt: Encryptor<Aes128>,
    },
}

impl<T> MCSocket<T> {
    pub fn new(stream: T) -> Self {
        Self::Cleartext { stream }
    }

    pub fn encrypt(mut self, key: &[u8; 16]) -> Result<Self, &str> {
        match self {
            MCSocket::Cleartext { stream } => {
                let decrypt = Decryptor::new_from_slices(key, key).map_err(|_| "Invalid key")?;
                let encrypt = Encryptor::new_from_slices(key, key).map_err(|_| "Invalid key")?;

                Ok(MCSocket::Encrypted {
                    stream,
                    decrypt,
                    encrypt,
                })
            }
            MCSocket::Encrypted { .. } => Err("Socket is already encrypted"),
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for MCSocket<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MCSocket::Cleartext { stream } => Pin::new(stream).poll_read(cx, buf),
            MCSocket::Encrypted {
                stream, decrypt, ..
            } => {
                let original_len = buf.filled().len();
                let poll = Pin::new(stream).poll_read(cx, buf);
                if let Poll::Ready(Ok(())) = poll {
                    // GenericArray
                    let bytes = buf.filled_mut()[original_len..]
                        .chunks_mut(Decryptor::<Aes128>::block_size());
                    for block in bytes {
                        decrypt.decrypt_block_mut(block.into())
                    }
                }

                poll
            }
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for MCSocket<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.get_mut() {
            MCSocket::Cleartext { stream } => Pin::new(stream).poll_write(cx, buf),
            MCSocket::Encrypted {
                stream, encrypt, ..
            } => todo!(),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            MCSocket::Cleartext { stream } => Pin::new(stream).poll_flush(cx),
            MCSocket::Encrypted { stream, .. } => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            MCSocket::Cleartext { stream } => Pin::new(stream).poll_shutdown(cx),
            MCSocket::Encrypted { stream, .. } => Pin::new(stream).poll_shutdown(cx),
        }
    }
}
