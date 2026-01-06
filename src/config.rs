use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_host: IpAddr,
    pub api_port: u16,
    pub chat_host: IpAddr,
    pub chat_port: u16,
    pub srs_host: IpAddr,
    pub srs_port: u16,
    pub base_path: PathBuf,
    pub banner_db_path: PathBuf,
    pub dump_path: PathBuf,
    pub secret_path: PathBuf,
    pub srs_api_host: String,
    pub srs_api_port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let base_path = PathBuf::from("/home/xbohodx02/work/rustyLive");

        Self {
            api_host: "127.0.0.1".parse().unwrap(),
            api_port: 3484,
            chat_host: "127.0.0.1".parse().unwrap(),
            chat_port: 3614,
            srs_host: "127.0.0.1".parse().unwrap(),
            srs_port: 8848,
            base_path: base_path.clone(),
            banner_db_path: base_path.join("config/bannerdb"),
            dump_path: base_path.join("dumps"),
            secret_path: base_path.join("secrets/secret.txt"),
            srs_api_host: "127.0.0.1".to_string(),
            srs_api_port: 1985,
        }
    }

    pub fn api_addr(&self) -> String {
        format!("{}:{}", self.api_host, self.api_port)
    }

    pub fn chat_addr(&self) -> String {
        format!("{}:{}", self.chat_host, self.chat_port)
    }

    pub fn srs_addr(&self) -> String {
        format!("{}:{}", self.srs_host, self.srs_port)
    }

    pub fn srs_api_url(&self) -> String {
        format!("http://{}:{}", self.srs_api_host, self.srs_api_port)
    }
}
