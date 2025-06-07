use pumpkin_data::packet::serverbound::STATUS_STATUS_REQUEST;
use spacetimemc_proxy_macros::packet;

#[derive(Default)]
#[packet(STATUS_STATUS_REQUEST)]
pub struct SStatusRequest;
