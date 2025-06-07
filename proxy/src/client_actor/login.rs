use crate::actor_ref::ActorRef;
use crate::client_actor::configuration::ConfigurationHandler;
use crate::client_actor::mc_socket;
use crate::client_actor::net::MCCodec;
use crate::client_actor::stream_actor::StreamActor;
use crate::server_actor::actor::{Server, ServerMessage};
use pumpkin::net::GameProfile;
use pumpkin_protocol::client::login::{CEncryptionRequest, CLoginSuccess};
use pumpkin_protocol::server::login::{SEncryptionResponse, SLoginAcknowledged, SLoginStart};
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, FramedParts};
use tokio_util::task::TaskTracker;

pub struct LoginHandler;

impl LoginHandler {
    pub async fn spawn(
        id: usize,
        client_address: SocketAddr,
        framer: Framed<TcpStream, MCCodec>,
        tracker: TaskTracker,
        server: Server,
    ) {
        let FramedParts {
            io: stream, codec, ..
        } = framer.into_parts();
        let login_actor = LoginActor {
            id,
            client_address,
            framer: Framed::from_parts(FramedParts::new(MCSocket::new(stream), codec)),
            server,
            tracker: tracker.clone(),
            profile: None,
            verify_token: None,
        };
        tracker.spawn(login_actor.run());
    }
}

impl Debug for LoginActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginActor")
            .field("id", &self.id)
            // .field("client_address", &self.client_address)
            .finish()
    }
}

type MCSocket = mc_socket::MCSocket<TcpStream>;

struct LoginActor {
    id: usize,
    client_address: SocketAddr,
    framer: Framed<MCSocket, MCCodec>,
    server: Server,
    tracker: TaskTracker,
    profile: Option<GameProfile>,
    verify_token: Option<[u8; 4]>,
}

impl StreamActor<Framed<MCSocket, MCCodec>> for LoginActor {
    fn get_stream(&mut self) -> &mut Framed<MCSocket, MCCodec> {
        &mut self.framer
    }
}

impl LoginActor {
    async fn run(mut self) {
        log::debug!("{self:?} initialized");

        let login = match self.read::<SLoginStart>().await {
            Some(login) => login,
            None => return self.shutdown().await,
        };

        if !self.handle_login_start(login).await {
            return self.shutdown().await;
        }

        let encryption_response = match self.read::<SEncryptionResponse>().await {
            Some(response) => response,
            None => return self.shutdown().await,
        };

        let secret = match self.handle_encryption_response(encryption_response).await {
            Some(secret) => secret,
            None => return self.shutdown().await,
        };

        let (ns, res) = self.encrypt(&secret);
        self = ns;
        if res.is_err() {
            log::error!("{self:?} failed to set encryption up : {res:?}");
            return self.shutdown().await;
        }

        log::info!("{self:?} encryption set up");

        // TODO : compression

        let profile = self.profile.take().unwrap();
        self.send(CLoginSuccess {
            uuid: &profile.id,
            username: &profile.name,
            // TODO : properties should contain skin info
            properties: &[],
        })
        .await;

        match self.read_empty::<SLoginAcknowledged>().await {
            Some(_) => {
                log::info!("{self:?} logged in as {}", profile.id);
                log::info!("{self:?} transitioning to configuration state");
                ConfigurationHandler::spawn(
                    self.id,
                    self.client_address,
                    self.framer,
                    self.tracker,
                    self.server,
                    profile,
                )
                .await
            }
            None => self.shutdown().await,
        }
    }

    fn encrypt(mut self, key: &[u8; 16]) -> (Self, Result<(), &str>) {
        let crypto = match MCSocket::prepare_encryption(key) {
            Ok(c) => c,
            Err(e) => {
                log::error!("{self:?} failed to prepare encryption : {e:?}");
                return (self, Err(e));
            }
        };
        let FramedParts {
            io: stream, codec, ..
        } = self.framer.into_parts();
        let (new_socket, res) = stream.encrypt(crypto);
        self.framer = Framed::from_parts(FramedParts::new(new_socket, codec));
        (self, res)
    }

    async fn handle_login_start(&mut self, login: SLoginStart) -> bool {
        log::debug!(
            "{:?} received login start : {} ({})",
            self,
            login.uuid,
            login.name
        );

        // TODO :
        //  - check there is free space
        //  - check username validity
        //  - support offline mode
        //  - velocity/bungeecord ?

        let der_recv = match self.server.ask(ServerMessage::CertificatePublicDer).await {
            Ok(der) => der,
            Err(_) => return false,
        };

        self.profile = Some(GameProfile {
            id: login.uuid,
            name: login.name,
            properties: vec![],
            profile_actions: None,
        });

        // Configure encryption ? (toggle)
        let verify_token = rand::random();
        self.verify_token = Some(verify_token);
        let der = match der_recv.await {
            Ok(der) => der,
            Err(_) => return false,
        };

        self.send(CEncryptionRequest {
            server_id: "",
            public_key: &der,
            verify_token: &verify_token,
            // TODO : authn with Mojang server
            should_authenticate: false,
        })
        .await
    }

    async fn handle_encryption_response(&self, response: SEncryptionResponse) -> Option<[u8; 16]> {
        log::debug!("{self:?} received encryption response");

        let verify_token_recv = match self
            .server
            .ask(|s| ServerMessage::Decrypt {
                data: response.verify_token,
                reply_to: s,
            })
            .await
        {
            Ok(recv) => recv,
            Err(_) => return None,
        };

        let shared_secret_recv = match self
            .server
            .ask(|s| ServerMessage::Decrypt {
                data: response.shared_secret,
                reply_to: s,
            })
            .await
        {
            Ok(recv) => recv,
            Err(_) => return None,
        };

        match verify_token_recv.await {
            Ok(token) => {
                if !token.eq(&self.verify_token.unwrap()) {
                    log::error!("{self:?} verify token mismatch !");
                    return None;
                }
            }
            Err(_) => {
                log::error!("{self:?} failed to decrypt verify token !");
                return None;
            }
        };

        let shared_secret = match shared_secret_recv.await {
            Ok(secret) => secret,
            Err(_) => {
                log::error!("{self:?} failed to decrypt shared secret !");
                return None;
            }
        };

        // TODO : authn with Mojang server

        <[u8; 16]>::try_from(&shared_secret[..16]).ok()
    }
}
