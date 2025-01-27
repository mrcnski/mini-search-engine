use anyhow::Context;
use axum::{routing::get, Router, Extension};
use std::sync::Arc;

mod consts;
mod crawler;
mod indexer;
mod routes;
mod utils;

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
    let app = Router::new().route("/", get(routes::index_handler))
        .layer(Extension(indexer));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", consts::PORT))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
