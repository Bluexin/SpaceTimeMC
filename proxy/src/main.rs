use log::error;
use pumpkin::{PumpkinServer, init_log, stop_server};
use pumpkin_data::packet::CURRENT_MC_PROTOCOL;
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::NamedColor;
use spacetimedb_sdk::{
    DbConnectionBuilder, DbContext, Error, Identity, SubscriptionHandle, Table, TableWithPrimaryKey,
};
use spacetimemc_proxy::module_bindings;
use spacetimemc_proxy::module_bindings::{
    BasicConfiguration, DbConnection, ErrorContext, ServerBasicConfigTableAccess,
    SubscriptionEventContext,
};
use spacetimemc_proxy::server_actor::CURRENT_MC_VERSION;
use spacetimemc_proxy::server_actor::actor::{Server, ServerMessage};
use std::io;
use std::sync::mpsc::channel;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio::time::sleep;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_VERSION: &str = env!("GIT_VERSION");

#[tokio::main]
async fn main() {
    let start_time = Instant::now();
    env_logger::builder().format_timestamp_millis().init();

    /*rayon::ThreadPoolBuilder::new()
    .thread_name(|_| "rayon-worker".to_string())
    .build_global()
    .expect("Rayon thread pool can only be initialized once");*/
    log::info!(
        "Starting SpaceTimeMC {CARGO_PKG_VERSION} ({GIT_VERSION}) for Minecraft {CURRENT_MC_VERSION} (Protocol {CURRENT_MC_PROTOCOL})",
    );

    log::debug!(
        "Build info: FAMILY: \"{}\", OS: \"{}\", ARCH: \"{}\", BUILD: \"{}\"",
        std::env::consts::FAMILY,
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
    );

    let db = connect_to_db();
    // db.run_async().await.expect("Unable to connect to database");
    let db_handle = db.run_threaded();
    // tokio::spawn(async { db.run_async().await });
    let mut config: Option<BasicConfiguration> = None;
    let sub_handle = subscribe_to_tables(&db);

    // TODO : hot reload of configs ?
    while config.is_none() {
        sleep(Duration::from_millis(20)).await;
        config = db.db.server_basic_config().iter().next();
    }

    let _config = &config.expect("Missing basic server configuration");
    /*let stserver = SpaceTimeServer::new(_config).await;
    stserver.init_plugins().await;*/
    let server_actor = Server::spawn(_config).await;

    let (death_sender, death_receiver) = oneshot::channel();
    server_actor
        .start(_config.server_address.clone(), death_sender)
        .await;

    log::info!(
        "Started server; took {}ms",
        start_time.elapsed().as_millis()
    );
    log::info!(
        "You can now connect to the server; listening on {}",
        _config.server_address // TODO : this doesn't handle automatic port
    );

    db.db
        .server_basic_config()
        .on_update(move |_ctx, old_row, new_row| {
            if new_row.id == 0 {
                log::debug!(
                    "Received update for server basic config : {} (was {})",
                    new_row.motd,
                    old_row.motd
                );
                server_actor
                    .try_send(ServerMessage::UpdateConfig {
                        config: new_row.clone(),
                    })
                    .unwrap_or_else(|e| {
                        log::error!("Failed to send update to server actor : {e:?}");
                    });
            }
        });

    let _ = death_receiver.await;

    log::info!("The server has stopped.");
    db.disconnect()
        .expect("Unable to cleanly disconnect from database.");
    db_handle.join().unwrap();
    log::info!("The database thread has stopped.");
}

fn subscribe_to_tables(ctx: &DbConnection) -> module_bindings::autogen::SubscriptionHandle {
    ctx.subscription_builder()
        .on_applied(on_sub_applied)
        .on_error(on_sub_error)
        .subscribe(["SELECT * FROM server_basic_config"])
}

fn on_sub_applied(ctx: &SubscriptionEventContext) {
    /*let basic_config = ctx.db.server_basic_config().iter().next()
    .expect("Missing basic server configuration");*/
    log::info!("Received subs");
    // Arc::make_mut(&mut config).replace(basic_config);
}

fn on_sub_error(_ctx: &ErrorContext, err: Error) {
    log::error!("Subscription failed: {}", err);
    // FIXME : use proper shutdown
    // std::process::exit(1);
}

fn handle_interrupt() {
    log::warn!(
        "{}",
        TextComponent::text("Received interrupt signal; stopping server...")
            .color_named(NamedColor::Red)
            .to_pretty_console()
    );
    stop_server();
}

/// The URI of the SpacetimeDB instance hosting our chat database and module.
const HOST: &str = "http://localhost:3000";

/// The database name we chose when we published our module.
const DB_NAME: &str = "spacetimemc";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        // Register our `on_connect` callback, which will save our auth token.
        .on_connect(on_connected)
        // Register our `on_connect_error` callback, which will print a message, then exit the process.
        .on_connect_error(on_connect_error)
        // Our `on_disconnect` callback, which will print a message, then exit the process.
        .on_disconnect(on_disconnected)
        // If the user has previously connected, we'll have saved a token in the `on_connect` callback.
        // In that case, we'll load it and pass it to `with_token`,
        // so we can re-authenticate as the same `Identity`.
        // .with_token(creds_store().load().expect("Error loading credentials"))
        // Set the database name we chose when we called `spacetime publish`.
        .with_module_name(DB_NAME)
        // Set the URI of the SpacetimeDB host that's running our database.
        .with_uri(HOST)
        // Finalize configuration and connect!
        .build()
        .expect("Failed to connect")
}

fn on_connected(_ctx: &DbConnection, _identity: Identity, token: &str) {
    log::info!("Connected to database as {_identity}");
    // FIXME : creds store and whatnot to ensure authenticity of client
    /*if let Err(e) = creds_store().save(token) {
        eprintln!("Failed to save credentials: {:?}", e);
    }*/
}

/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    log::error!("Connection error: {:?}", err);
    // std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        log::error!("STDB disconnected with error: {}", err);
        // std::process::exit(1);
    } else {
        log::info!("STDB disconnected without error.");
        // std::process::exit(0);
    }
}
