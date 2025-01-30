// TODO: These consts could probably be moved to their respective modules.

pub const NAME: &str = "Mini Search Engine";
pub const NEW_INDEX: bool = false;
pub const PORT: u16 = 3000;

// Crawler consts.
// TODO: Update these values.
pub const DOMAINS_FILE: &str = "domains";
pub const LOG_INTERVAL_PER_DOMAIN: u32 = 20;
pub const MAX_PAGES_PER_DOMAIN: u32 = 100;

// Indexer consts.
pub const DB_NAME: &str = "stats.db";
pub const SEARCH_INDEX_DIR: &str = "search_index";
pub const RESULTS_PER_QUERY: usize = 10;
