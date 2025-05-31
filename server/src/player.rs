use crate::types_support::UUID;
use spacetimedb::{ReducerContext, Table, Timestamp, reducer, table};
use std::fmt::{Debug, Formatter};
use std::str::FromStr;
use uuid::Uuid;

#[table(name = player)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    entity_id: u32,
    last_known_username: Option<String>,
    #[unique]
    profile_id: u128,
    online: bool,
    last_seen: Timestamp,
}

impl Player {
    pub fn new(username: String, profile_id: u128, now: Timestamp) -> Self {
        Self {
            entity_id: 0,
            last_known_username: Some(username),
            profile_id,
            online: false,
            last_seen: now,
        }
    }
}

impl Debug for Player {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("entity_id", &self.entity_id)
            .field("last_known_username", &self.last_known_username)
            .field("profile_id", &Uuid::from_u128(self.profile_id))
            .field("online", &self.online)
            .field("last_seen", &self.last_seen)
            .finish()
    }
}

#[reducer]
fn add_player(
    ctx: &ReducerContext,
    username: String,
    profile_id_str: String,
) -> Result<(), String> {
    let profile_id = UUID::from_str(&profile_id_str)?.as_u128();
    let player = if let Some(player) = ctx.db.player().profile_id().find(profile_id) {
        ctx.db.player().entity_id().update(Player {
            last_known_username: Some(username),
            last_seen: ctx.timestamp,
            ..player
        })
    } else {
        ctx.db
            .player()
            .insert(Player::new(username, profile_id, ctx.timestamp))
    };
    log::info!("Upserted player: {:?}", player);

    Ok(())
}
