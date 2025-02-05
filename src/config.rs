use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub crawler: CrawlerConfig,
    pub indexer: IndexerConfig,
}

/// Server settings
#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub results_per_query: usize,
}

/// Crawler settings
#[derive(Clone, Debug, Deserialize)]
pub struct CrawlerConfig {
    pub domains_file: String,
    pub log_interval_per_domain: u32,
    pub max_pages_per_domain: u32,
}

/// Indexer settings
#[derive(Clone, Debug, Deserialize)]
pub struct IndexerConfig {
    pub new_index: bool,
    pub index_dir: String,
    pub db_dir: String,
    pub commit_interval_ms: u64,
    pub tech_term_boost: f32,
}

impl Config {
    #[cfg(not(test))]
    pub fn load() -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string("config.yaml")?;
        let config: Config = serde_yaml::from_str(&config_str)?;
        Ok(config)
    }
    #[cfg(test)]
    pub fn load() -> anyhow::Result<Self> {
        unimplemented!("load() should not be called in tests, see load_test()");
    }

    #[cfg(test)]
    pub fn load_test(test_name: &str) -> Self {
        use crate::test_utils::TEST_DIR;

        Config {
            server: ServerConfig {
                name: "test_server".to_string(),
                results_per_query: 10,
            },
            crawler: CrawlerConfig {
                domains_file: format!("{TEST_DIR}/test_domains"),
                log_interval_per_domain: 1,
                max_pages_per_domain: 1,
            },
            indexer: IndexerConfig {
                new_index: true,
                index_dir: format!("{TEST_DIR}/index_{test_name}"),
                db_dir: format!("{TEST_DIR}/db_{test_name}.db"),
                commit_interval_ms: 1000,
                tech_term_boost: 1.0,
            },
        }
    }
}
