use pumpkin_util::GameMode as PumpkinGameMode;
use std::path::PathBuf;

pub mod autogen;
pub use autogen::*;

pub trait GetWorldPath {
    fn get_world_path(&self) -> PathBuf;
}

impl GetWorldPath for BasicConfiguration {
    fn get_world_path(&self) -> PathBuf {
        format!("./{}", self.default_level_name).parse().unwrap()
    }
}

impl From<GameMode> for PumpkinGameMode {
    fn from(value: GameMode) -> Self {
        match value {
            GameMode::Survival => PumpkinGameMode::Survival,
            GameMode::Creative => PumpkinGameMode::Creative,
            GameMode::Adventure => PumpkinGameMode::Adventure,
            GameMode::Spectator => PumpkinGameMode::Spectator,
        }
    }
}
