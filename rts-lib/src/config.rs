use std::fs;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(default="ScreenConfig::default_screen")]
    screen: ScreenConfig
}

impl Config {
    fn default_config() -> Config {
        Config {
            screen: ScreenConfig::default_screen()
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ScreenConfig {
    #[serde(default)]
    full_screen: bool,
    #[serde(default="ScreenConfig::default_width")]
    width: i32,
    #[serde(default="ScreenConfig::default_height")]
    height: i32,
}

impl ScreenConfig {
    fn default_screen() -> ScreenConfig {
        ScreenConfig {
            full_screen: false,
            width: ScreenConfig::default_width(),
            height: ScreenConfig::default_height()
        }
    }

    fn default_width() -> i32 { 800 }
    fn default_height() -> i32 { 600 }
}

const CONFIG_PATH: &str = "./config.toml";

pub fn read_config() -> Config {
    if Path::new(CONFIG_PATH).exists()
    {
        let toml_str = fs::read_to_string(CONFIG_PATH).expect("read config file");
        toml::from_str(&toml_str).expect("parse config")
    }
    else
    {
        Config::default_config()
    }
}

pub fn write_config(config: &Config) {
    let toml_str = toml::to_string(config).expect("serialize config");
    fs::write(CONFIG_PATH, toml_str).expect("write config file")
}
