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
        .context("Failed to start background indexer")?;

    if CONFIG.indexer.new_index {
        crawler::initial_crawl(indexer_tx).await.context("Failed to do initial crawl")?;
    }

    run_server(indexer).await.context("Failed to run server")
}

async fn run_server(indexer: Arc<Indexer>) -> anyhow::Result<()> {
    println!("Server starting on http://localhost:{}", CONFIG.server.port);

    let app = routes::create_router(indexer);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", CONFIG.server.port))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
