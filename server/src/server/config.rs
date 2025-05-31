use spacetimedb::{ReducerContext, SpacetimeType, reducer};

#[derive(PartialEq, Clone, Debug, SpacetimeType)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, SpacetimeType)]
pub enum PermissionLvl {
    #[default]
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, SpacetimeType)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

#[spacetimedb::table(name = server_basic_config, public)]
pub struct BasicConfiguration {
    #[primary_key]
    pub id: u8,
    /// The address to bind the server to.
    pub server_address: String,
    /// The seed for world generation.
    pub seed: String,
    /// The maximum number of players allowed on the server. Specifying `0` disables the limit.
    pub max_players: u32,
    /// The maximum view distance for players.
    pub view_distance: u8,
    /// The maximum simulated view distance.
    pub simulation_distance: u8,
    /// The default game difficulty.
    pub default_difficulty: Difficulty,
    /// The op level assigned by the /op command
    pub op_permission_level: PermissionLvl,
    /// Whether the Nether dimension is enabled.
    pub allow_nether: bool,
    /// Whether the server is in hardcore mode.
    pub hardcore: bool,
    /// Whether online mode is enabled. Requires valid Minecraft accounts.
    pub online_mode: bool,
    /// Whether packet encryption is enabled. Required when online mode is enabled.
    pub encryption: bool,
    /// Message of the Day; the server's description displayed on the status screen.
    pub motd: String,
    /// The server's ticks per second.
    pub tps: f32,
    /// The default gamemode for players.
    pub default_gamemode: GameMode,
    /// If the server force the gamemode on join
    pub force_gamemode: bool,
    /// Whether to remove IPs from logs or not
    pub scrub_ips: bool,
    /// Whether to use a server favicon
    pub use_favicon: bool,
    /// Path to server favicon
    pub favicon_path: String,
    /// The default level name
    pub default_level_name: String,
    /// Whether chat messages should be signed or not
    pub allow_chat_reports: bool,
    /// Whether to enable the whitelist
    pub white_list: bool,
    /// Whether to enforce the whitelist
    pub enforce_whitelist: bool,
}

impl Default for BasicConfiguration {
    fn default() -> Self {
        Self {
            id: 0,
            server_address: "0.0.0.0:25565".into(),
            seed: "".to_string(),
            max_players: 100_000,
            view_distance: 10,
            simulation_distance: 10,
            default_difficulty: Difficulty::Normal,
            op_permission_level: PermissionLvl::Four,
            allow_nether: true,
            hardcore: false,
            online_mode: true,
            encryption: true,
            motd: "A blazingly fast SpaceTimeMC server!".into(),
            tps: 20.0,
            default_gamemode: GameMode::Creative, // easier for WIP
            force_gamemode: false,
            scrub_ips: true,
            use_favicon: true,
            favicon_path: "icon.png".into(),
            default_level_name: "world".into(),
            allow_chat_reports: false,
            white_list: false,
            enforce_whitelist: false,
        }
    }
}

#[reducer]
fn update_motd(ctx: &ReducerContext, motd: String) -> Result<(), String> {
    match ctx.db.server_basic_config().id().find(0) {
        Some(mut config) => {
            config.motd = motd;
            ctx.db.server_basic_config().id().update(config);
            Ok(())
        }
        None => Err("Did not find a basic config !".into()),
    }
}
