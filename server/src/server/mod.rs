pub mod config;
use config::*;

use spacetimedb::{ReducerContext, Table};

pub fn init(ctx: &ReducerContext) {
    log::info!("Initializing basic configuration from {}", ctx.sender);
    if ctx.db.server_basic_config().count() == 0 {
        ctx.db
            .server_basic_config()
            .insert(BasicConfiguration::default());
        log::info!("Stored default basic configuration");
    }
}
