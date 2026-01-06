pub mod srs;
pub mod chat;
pub mod banner;

pub use srs::{SrsDatabase, ClientStatus, StreamerStatus};
pub use chat::{ChatDatabaseInner, ChatEntry};
pub use banner::BannerDatabase;

use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::Config;

/// Global application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub srs_db: Arc<RwLock<srs::SrsDatabaseInner>>,
    pub chat_db: Arc<RwLock<chat::ChatDatabaseInner>>,
    pub banner_db: Arc<BannerDatabase>,
    pub config: Config,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let banner_db = Arc::new(BannerDatabase::new(&config.banner_db_path)?);
        let secret_path = config.secret_path.clone();
        let dump_path = config.dump_path.clone();

        Ok(Self {
            srs_db: Arc::new(RwLock::new(srs::SrsDatabaseInner::new(secret_path)?)),
            chat_db: Arc::new(RwLock::new(chat::ChatDatabaseInner::new(dump_path))),
            banner_db,
            config,
        })
    }
}
