// TODO: These consts could probably be moved to their respective modules.

pub const PORT: u16 = 3000;
pub const NAME: &str = "Mini Search Engine";

// Crawler consts.
// TODO: Update these values.
pub const DOMAINS_FILE: &str = "domains";
pub const LOG_INTERVAL_PER_DOMAIN: u32 = 10;
pub const MAX_PAGES_PER_DOMAIN: u32 = 15;

// Indexer consts.
pub const DB_NAME: &str = "stats.db";
pub const SEARCH_INDEX_DIR: &str = "search_index";
pub const RESULTS_PER_QUERY: usize = 10;
