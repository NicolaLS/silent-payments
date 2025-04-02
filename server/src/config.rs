use std::env;

use crate::{Error, Result};

pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub syncer: SyncerConfig,
}

pub struct ServerConfig {
    pub server_host: String,
    pub server_port: String,
}

pub struct DatabaseConfig {
    pub database_url: String,
}

pub struct SyncerConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_pass: String,
    pub sync_from: i64,
}

impl Config {
    pub fn try_from_env() -> Result<Self> {
        dotenvy::dotenv().map_err(|_| Error::Config)?;

        Ok(Self {
            server: ServerConfig {
                server_host: get_env("SERVER_HOST")?,
                server_port: get_env("SERVER_PORT")?,
            },
            database: DatabaseConfig {
                database_url: get_env("DATABASE_URL")?,
            },
            syncer: SyncerConfig {
                rpc_url: get_env("RPC_URL")?,
                rpc_user: get_env("RPC_USER")?,
                rpc_pass: get_env("RPC_PASS")?,
                sync_from: get_env("SYNC_FROM")?
                    .parse::<i64>()
                    .map_err(|_| Error::Config)?,
            },
        })
    }
}

fn get_env(key: &str) -> Result<String> {
    env::var(key).map_err(|_| Error::Config)
}
