// Set allocator.
use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Context;
use std::sync::Arc;

mod consts;
mod crawler;
mod indexer;
mod routes;

use indexer::Indexer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start the background indexing task. Periodically commits.
    let (indexer, indexer_tx) = indexer::start()
        .await
        .context("Failed to start background indexer")?;

    crawler::initial_crawl(indexer_tx).await.context("Failed to crawl")?;

    println!("Server starting on http://localhost:{}", consts::PORT);
    run_server(indexer).await.context("Failed to run server")?;

    Ok(())
}

async fn run_server(indexer: Arc<Indexer>) -> anyhow::Result<()> {
    let app = routes::create_router(indexer);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", consts::PORT))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
