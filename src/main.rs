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

use config::CONFIG;
use indexer::Indexer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start the background indexing task. Periodically commits.
    let (indexer, indexer_tx) = indexer::start(CONFIG.indexer.new_index)
        .await
        .context("Failed to start indexer")?;

    if CONFIG.indexer.new_index {
        crawler::initial_crawl(indexer_tx).await.context("Failed to do initial crawl")?;
    }

    run_server(indexer).await.context("Failed to run server")
}

async fn run_server(indexer: Arc<Indexer>) -> anyhow::Result<()> {
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let app = routes::create_router(indexer);

    println!("Server starting on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
