use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use once_cell::sync::Lazy;
use rendering::GraphicsSettings;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub graphics: GraphicsSettings,
}

pub static CONFIG: Lazy<Config> = Lazy::new(Config::new);

impl Config {
    fn new() -> Config {
        let cfg = engine::filesystem::DIRS.project.config_dir();
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(cfg.join("game_settings.toml")))
            .merge(Env::prefixed("DRAGONFIRE_"))
            .extract().expect("Failed to load settings")
    }
}