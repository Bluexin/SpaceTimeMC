use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, BlockSizeUser, KeyIvInit};
use aes::Aes128;
use cfb8::{Decryptor, Encryptor};
use pin_project_lite::pin_project;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pin_project! {
    #[project = MCSocketProjection]
    pub enum MCSocket<T> {
        Cleartext {
            #[pin]
            stream: T
        },
        Encrypted {
            #[pin]
            stream: T,
            decrypt: Box<Decryptor<Aes128>>,
            encrypt: Box<Encryptor<Aes128>>,
            // TODO : maybe keep some sort of buffer here?
            pending_bytes: Option<Box<[u8]>>,
        }
    }
}

type CryptoPair = (Decryptor<Aes128>, Encryptor<Aes128>);

impl<T> MCSocket<T> {
    pub fn new(stream: T) -> Self {
        Self::Cleartext { stream }
    }

    pub fn prepare_encryption(key: &[u8; 16]) -> Result<CryptoPair, &str> {
        let decrypt = Decryptor::new_from_slices(key, key).map_err(|_| "Invalid key")?;
        let encrypt = Encryptor::new_from_slices(key, key).map_err(|_| "Invalid key")?;
        Ok((decrypt, encrypt))
    }

    pub fn encrypt(self, crypto: CryptoPair) -> (Self, Result<(), &'static str>) {
        match self {
            MCSocket::Cleartext { stream } => {
                let (decrypt, encrypt) = crypto;

                (
                    MCSocket::Encrypted {
                        stream,
                        decrypt: decrypt.into(),
                        encrypt: encrypt.into(),
                        pending_bytes: None,
                    },
                    Ok(()),
                )
            }
            MCSocket::Encrypted { .. } => (self, Err("Socket is already encrypted")),
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for MCSocket<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            MCSocketProjection::Cleartext { stream } => stream.poll_read(cx, buf),
            MCSocketProjection::Encrypted {
                stream, decrypt, ..
            } => {
                let original_len = buf.filled().len();
                let poll = stream.poll_read(cx, buf);
                if let Poll::Ready(Ok(())) = poll {
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
        match self.project() {
            MCSocketProjection::Cleartext { stream } => stream.poll_write(cx, buf),
            MCSocketProjection::Encrypted {
                stream,
                encrypt,
                pending_bytes,
                ..
            } => {
                let (mut out, already_encrypted) = if let Some(remaining) = pending_bytes.take() {
                    let len = remaining.len();
                    let mut r = remaining.to_vec();
                    r.reserve(buf.len() - len);
                    (r, len)
                } else {
                    (Vec::with_capacity(buf.len()), 0)
                };

                // TODO : can we bulk encrypt?
                let mut tmp_out = [0u8];
                for block in buf[already_encrypted..].chunks(Decryptor::<Aes128>::block_size()) {
                    // This is a stream cipher, so this value must be used
                    let out_block = GenericArray::from_mut_slice(&mut tmp_out);
                    encrypt.encrypt_block_b2b_mut(block.into(), out_block);
                    out.extend_from_slice(&tmp_out[..]);
                }

                match stream.poll_write(cx, &out) {
                    Poll::Pending => {
                        let _ = pending_bytes.insert(out[..].into());
                        Poll::Pending
                    }
                    Poll::Ready(result) => match result {
                        Ok(written) => {
                            if written < out.len() {
                                let _ = pending_bytes.insert(out[written..].into());
                            }
                            Poll::Ready(Ok(written))
                        }
                        Err(err) => Poll::Ready(Err(err)),
                    },
                }
            }
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
