use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub crawler: CrawlerConfig,
    pub indexer: IndexerConfig,
}

/// Server settings
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
}

/// Crawler settings
#[derive(Debug, Deserialize)]
pub struct CrawlerConfig {
    pub domains_file: String,
    pub log_interval_per_domain: u32,
    pub max_pages_per_domain: u32,
}

/// Indexer settings
#[derive(Debug, Deserialize)]
pub struct IndexerConfig {
    pub new_index: bool,
    pub commit_interval_ms: u64,
    pub db_name: String,
    pub search_index_dir: String,
    pub results_per_query: usize,
    pub tech_term_boost: f32,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_str = fs::read_to_string("config.yaml")?;
        let config: Config = serde_yaml::from_str(&config_str)?;
        Ok(config)
    }
}

lazy_static::lazy_static! {
    pub static ref CONFIG: Config = Config::load().expect("Failed to load config");
}
