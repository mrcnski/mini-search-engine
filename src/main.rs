// Set allocator.
use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Context;
use std::sync::Arc;

mod config;
mod crawler;
mod indexer;
mod routes;
#[cfg(test)]
mod test_utils;

use config::Config;
use indexer::Indexer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load().context("Failed to load config")?;

    // Start the background indexing task. Periodically commits.
    let (indexer, indexer_tx) = indexer::start(&config.indexer)
        .await
        .context("Failed to start indexer")?;

    if config.indexer.new_index {
        crawler::initial_crawl(indexer_tx, &config.crawler)
            .await
            .context("Failed to do initial crawl")?;
    }

    run_server(indexer, &config).await.context("Failed to run server")
}

async fn run_server(indexer: Arc<Indexer>, config: &Config) -> anyhow::Result<()> {
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let app = routes::create_router(indexer, &config.server);

    println!("Server starting on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
