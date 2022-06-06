use std::fs::File;

use figment::providers::{Env, Format, Serialized, Yaml};
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
            .merge(Yaml::file(cfg.join("game_settings.yaml")))
            .merge(Env::prefixed("DRAGONFIRE_"))
            .extract()
            .expect("Failed to load settings")
    }

    pub fn save(&self) {
        let cfg = DIRS.project.config_dir().join("game_settings.toml");
        if let Err(e) = File::open(&cfg).map(|file| serde_yaml::to_writer(file, self)) {
            error!("Could not save config file: {e}");
        } else {
            info!("Saved config file to {cfg:?}");
        }
    }
}
