use std::fs::File;

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use log::{error, info};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use engine::filesystem::DIRS;
use rendering::GraphicsSettings;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub graphics: GraphicsSettings,
    pub log_level: String,
}

pub static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| RwLock::new(Config::new()));

impl Config {
    fn new() -> Config {
        let cfg = DIRS.project.config_dir();
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(cfg.join("game_settings.toml")))
            .merge(Env::prefixed("DRAGONFIRE_"))
            .extract()
            .expect("Failed to load settings")
    }

    pub fn save(&self) {
        let cfg = DIRS.project.config_dir().join("game_settings.toml");
        match toml::to_string(self) {
            Ok(str) => if let Err(e) = std::fs::write(&cfg, str) {
                error!("Error writing config file: {e}");
            },
            Err(e) => error!("Error serializing config: {e}")
        }
    }
}
