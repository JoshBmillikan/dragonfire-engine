use figment::providers::{Env, Format, Serialized, Toml, Yaml};
use figment::Figment;
use log::error;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use engine::filesystem::DIRS;
use rendering::GraphicsSettings;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Config {
    pub graphics: GraphicsSettings,
    pub log_level: String,
}

pub static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| RwLock::new(Config::new()));

impl Config {
    fn new() -> Config {
        let cfg = DIRS.project.config_dir();
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(cfg.join("engine_settings.toml")))
            .merge(Yaml::file(cfg.join("engine_settings.yaml")))
            .merge(Env::prefixed("DRAGONFIRE_"))
            .extract()
            .expect("Failed to load settings")
    }

    pub fn save(&self) {
        let cfg = DIRS.project.config_dir().join("engine_settings.yaml");
        match serde_yaml::to_string(self) {
            Ok(str) => if let Err(e) = std::fs::write(&cfg, str) {
                error!("Error writing config file: {e}");
            },
            Err(e) => error!("Error serializing config: {e}")
        }
    }
}

#[cfg(test)]
mod test {
    use crate::config::Config;

    #[test]
    fn config_serialization() {
        let cfg = Config::default();
        let string = serde_yaml::to_string(&cfg).expect("Failed to serialize config");
        let result: Config = serde_yaml::from_str(&string).expect("Failed to deserialize config");
        assert_eq!(result, cfg);
    }
}