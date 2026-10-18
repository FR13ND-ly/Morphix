pub mod db;
pub mod state;

use std::{env, net::SocketAddr};

pub struct Config {
    pub address: SocketAddr,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            address: value_or_default("APP_ADDRESS", "127.0.0.1:3000")?.parse()?,
            database_url: value_or_default("DATABASE_URL", "sqlite://cellab.db")?,
        })
    }
}

fn value_or_default(key: &str, default: &str) -> Result<String, env::VarError> {
    match env::var(key) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(error) => Err(error),
    }
}
