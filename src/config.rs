use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub discord_token: String,
    pub discord_release_channel_id: u64,
    #[serde(default = "default_port")]
    pub api_port: u16,
    pub api_key: String,
    pub guild_id: u64,
}

fn default_port() -> u16 {
    8080
}

impl Config {
    pub fn from_env() -> Result<Self, envy::Error> {
        dotenvy::dotenv().ok();
        envy::from_env()
    }
}
