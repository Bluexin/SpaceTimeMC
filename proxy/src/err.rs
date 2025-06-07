use std::fmt::Debug;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum TrySendError {
    ChannelFull,
    ChannelClosed,
}

impl<T> From<mpsc::error::TrySendError<T>> for TrySendError
where
    T: Debug,
{
    fn from(value: mpsc::error::TrySendError<T>) -> Self {
        match value {
            mpsc::error::TrySendError::Full(message) => {
                log::error!("Channel full while sending : {message:?}");
                TrySendError::ChannelFull
            }
            mpsc::error::TrySendError::Closed(message) => {
                log::error!("Channel closed while sending : {message:?}");
                TrySendError::ChannelClosed
            }
        }
    }
}

#[derive(Debug)]
pub struct SendError;

impl<T> From<mpsc::error::SendError<T>> for SendError
where
    T: Debug,
{
    fn from(value: mpsc::error::SendError<T>) -> Self {
        log::error!("Channel closed while sending : {:?}", value.0);
        SendError
    }
}
