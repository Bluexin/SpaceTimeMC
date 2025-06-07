use crate::actor_ref::ActorRef;
use crate::client_actor::handshake::HandshakeHandler;
use crate::err::{SendError, TrySendError};
use crate::module_bindings::autogen::BasicConfiguration;
use crate::server_actor::connection_cache::CachedStatus;
use crate::server_actor::key_store::KeyStore;
use pumpkin::net::authentication::fetch_mojang_public_keys;
use pumpkin_config::advanced_config;
use rsa::RsaPublicKey;
use std::default::Default;
use std::num::Wrapping;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::select;
use tokio::signal::unix::{signal, Signal, SignalKind};
use tokio::sync::{mpsc, oneshot};
use tokio_util::task::TaskTracker;

pub struct Server {
    sender: mpsc::Sender<ServerMessage>,
}

impl Server {
    pub async fn spawn(basic_configuration: &BasicConfiguration) -> Self {
        let (sender, receiver) = mpsc::channel(16);

        let actor = ServerActor::new(basic_configuration, receiver, sender.clone()).await;
        tokio::spawn(actor.run());

        Self { sender }
    }

    pub async fn start(&self, address: String, death: oneshot::Sender<()>) {
        self.send(ServerMessage::StartListener { address, death })
            .await
            // If we already fail to send when starting the server, might as well panic
            .unwrap();
    }

    fn from_address(sender: mpsc::Sender<ServerMessage>) -> Self {
        Self { sender }
    }
}

impl ActorRef<ServerMessage> for Server {
    async fn send(&self, msg: ServerMessage) -> Result<(), SendError> {
        self.sender.send(msg).await.map_err(SendError::from)
    }

    fn try_send(&self, msg: ServerMessage) -> Result<(), TrySendError> {
        self.sender.try_send(msg).map_err(TrySendError::from)
    }
}

#[derive(Debug)]
pub enum ServerMessage {
    Shutdown,
    GetStatus(oneshot::Sender<String>),
    StartListener {
        address: String,
        death: oneshot::Sender<()>,
    },
    UpdateConfig {
        config: BasicConfiguration,
    },
    CertificatePublicDer(oneshot::Sender<Box<[u8]>>),
    Decrypt {
        data: Box<[u8]>,
        reply_to: oneshot::Sender<Vec<u8>>,
    },
}

/// Actor for the server
struct ServerActor {
    listing: CachedStatus,
    // connections: Vec<Connection>,
    message_receiver: mpsc::Receiver<ServerMessage>,
    auth_client: Option<reqwest::Client>,
    mojang_public_keys: Option<Vec<RsaPublicKey>>,
    use_whitelist: bool,
    tasks: TaskTracker,
    self_addr: mpsc::Sender<ServerMessage>,
    key_store: KeyStore,
}

impl ServerActor {
    async fn new(
        basic_configuration: &BasicConfiguration,
        message_receiver: mpsc::Receiver<ServerMessage>,
        self_addr: mpsc::Sender<ServerMessage>,
    ) -> Self {
        let auth_client = Self::auth_client(basic_configuration);

        Self {
            listing: CachedStatus::from_config(basic_configuration),
            // connections: Vec::new(),
            message_receiver,
            mojang_public_keys: if let Some(client) = &auth_client {
                Self::mojang_pubkeys(basic_configuration, client).await
            } else {
                None
            },
            auth_client,
            use_whitelist: basic_configuration.white_list,
            tasks: TaskTracker::new(),
            self_addr,
            key_store: Default::default(),
        }
    }

    fn auth_client(basic_configuration: &BasicConfiguration) -> Option<reqwest::Client> {
        basic_configuration.online_mode.then(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_millis(u64::from(
                    advanced_config().networking.authentication.connect_timeout,
                )))
                .read_timeout(Duration::from_millis(u64::from(
                    advanced_config().networking.authentication.read_timeout,
                )))
                .build()
                .expect("Failed to to make reqwest client")
        })
    }

    async fn mojang_pubkeys(
        basic_configuration: &BasicConfiguration,
        auth_client: &reqwest::Client,
    ) -> Option<Vec<RsaPublicKey>> {
        if basic_configuration.allow_chat_reports {
            match fetch_mojang_public_keys(auth_client).await {
                Ok(pub_keys) => Some(pub_keys),
                Err(err) => {
                    log::error!(
                        "Failed to fetch mojang public keys: {err:?}, server will not allow chat reports"
                    );
                    None
                }
            }
        } else {
            None
        }
    }

    async fn run(mut self) {
        // TODO : select between message_receiver and packet_receiver
        while let Some(msg) = self.message_receiver.recv().await {
            self.handle_message(msg).await;
        }
    }

    // Note on usage of `let _ =` :
    // In this case don't mind if the connection already died, the server can safely ignore it at
    // this stage
    async fn handle_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Shutdown => self.shutdown().await,
            ServerMessage::GetStatus(reply_to) => {
                let _ = reply_to.send(self.listing.get_status_string());
            }
            ServerMessage::StartListener { address, death } => {
                self.start_listener(address, death).await
            }
            ServerMessage::UpdateConfig { config } => {
                // TODO : this should update rather than override (to keep player count)
                // Might not even need to be cached at all
                self.listing = CachedStatus::from_config(&config);
            }
            ServerMessage::CertificatePublicDer(reply_to) => {
                let _ = reply_to.send(self.key_store.get_public_der().into());
            }
            ServerMessage::Decrypt { data, reply_to } => {
                match self.key_store.decrypt(&data) {
                    Ok(decrypted) => {
                        let _ = reply_to.send(decrypted);
                    }
                    Err(e) => {
                        log::error!("Failed to decrypt: {e:?}");
                        // this drops reply_to, closing it
                    }
                }
            }
        }
    }

    async fn shutdown(&mut self) {
        log::debug!("Shutting down server");
        /*for connection in &self.connections {
            // If we failed to terminate the connection, it is probably dead anyway
            let _ = connection.send(ConnectionMessage::Shutdown).await;
        }*/
        log::debug!("Closed all connections");
        self.tasks.close();
        log::debug!("Awaiting tasks for server");
        self.tasks.wait().await;
        log::debug!("Done awaiting tasks for server");
        // This will stop the run loop
        self.message_receiver.close();
        log::debug!("Closed message receiver");
    }

    async fn start_listener(&self, server_address: String, death: oneshot::Sender<()>) {
        let tasks = TaskTracker::new();
        tasks.spawn(Self::run_listener(
            server_address,
            tasks.clone(),
            self.self_addr.clone(),
            death,
        ));
    }

    async fn run_listener(
        server_address: String,
        tasks: TaskTracker,
        self_addr: mpsc::Sender<ServerMessage>,
        death: oneshot::Sender<()>,
    ) {
        let mut master_client_id: Wrapping<usize> = Wrapping(0);
        let (mut sigint, mut sighup, mut sigterm) = Self::signals();
        let listener = TcpListener::bind(server_address)
            .await
            .expect("Failed to start `TcpListener`");

        loop {
            match select! {
                biased;
                _ = sigint.recv() => None,
                _ = sighup.recv() => None,
                _ = sigterm.recv() => None,
                open = listener.accept() => Some(open),
            } {
                Some(Ok((connection, client_addr))) => {
                    let id = master_client_id.0;
                    master_client_id += 1;
                    log::info!("Accepting connection from: {client_addr} (id {id})");
                    HandshakeHandler::spawn(
                        connection,
                        client_addr,
                        id,
                        &tasks,
                        Server::from_address(self_addr.clone()),
                    );
                }
                Some(Err(e)) => {
                    log::error!("Failed to accept connection: {e}");
                    break;
                }
                None => {
                    log::info!("Got none (interrupt)");
                    break;
                }
            }
        }

        log::info!("Server actor death");
        death.send(()).unwrap()
    }

    // TODO : these are Unix specific, on Win it should be ctrl_c / ctrl_break / ctrl_close / ctrl_shutdown
    // See https://github.com/Finomnis/tokio-graceful-shutdown and https://stackoverflow.com/a/77591939
    fn signals() -> (Signal, Signal, Signal) {
        let sigint = signal(SignalKind::interrupt()).expect("Unable to set interrupt handler");
        let sighup = signal(SignalKind::hangup()).expect("Unable to set hangup handler");
        let sigterm = signal(SignalKind::terminate()).expect("Unable to set terminate handler");
        (sigint, sighup, sigterm)
    }
}
