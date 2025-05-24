use spacetimedb::{reducer, ReducerContext};

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    log::info!("Initialized : {}", ctx.sender);
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    log::info!("Client connected : {}", ctx.sender);
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    log::info!("Client disconnected : {}", ctx.sender);
}
