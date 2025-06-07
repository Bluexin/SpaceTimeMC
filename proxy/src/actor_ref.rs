use crate::err::{SendError, TrySendError};
use futures::FutureExt;
use tokio::sync::oneshot;

pub trait ActorRef<Message> {
    fn send(&self, msg: Message) -> impl Future<Output = Result<(), SendError>>;

    fn try_send(&self, msg: Message) -> Result<(), TrySendError>;

    /// Typically used via tuple enum reference :
    /// ```rs
    /// let recv = actor.ask(ActorMessage::GetSomething).await
    ///     .expect("Actor closed channel");
    /// let response = recv.await;
    /// ```
    /// But can also be used with struct enums :
    /// ```rs
    /// let recv = actor.ask(|reply_to| ActorMessage::GetSomethingComplex {
    ///     reply_to, seed: 42
    /// }).await.expect("Actor closed channel");
    /// let response = recv.await;
    /// ```
    ///
    /// This function is async due to async usage of `send`.
    /// As outlined in the examples above, users should `await` the result of this function call
    /// to get the [oneshot::Receiver<T>] which when `await`ed will give you the answer you
    /// are looking for.
    ///
    /// This will return a [SendError] if the the actor closed the channel.
    fn ask<T, F>(&self, what: F) -> impl Future<Output = Result<oneshot::Receiver<T>, SendError>>
    where
        F: FnOnce(oneshot::Sender<T>) -> Message,
    {
        let (send, recv) = oneshot::channel();
        self.send(what(send)).map(|r| r.map(|_| recv))
    }

    fn try_ask<T, F>(&self, what: F) -> Result<oneshot::Receiver<T>, TrySendError>
    where
        F: FnOnce(oneshot::Sender<T>) -> Message,
    {
        let (send, recv) = oneshot::channel();
        self.try_send(what(send))?;
        Ok(recv)
    }
}
