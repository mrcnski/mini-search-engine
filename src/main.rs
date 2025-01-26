use anyhow::Context;
use axum::{routing::get, Router};

mod consts;
mod crawler;
mod indexer;
mod routes;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start the background indexing task. Periodically commits.
    let indexer_tx = indexer::start()
        .await
        .context("Failed to start background indexer")?;

    // TODO: time this.
    crawler::initial_crawl(indexer_tx).await.context("Failed to crawl")?;

    println!("Server starting on http://localhost:{}", consts::PORT);
    run_server().await.context("Failed to run server")?;

    Ok(())
}

async fn run_server() -> anyhow::Result<()> {
    let app = Router::new().route("/", get(routes::index_handler));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", consts::PORT))
        .await
        .context("Failed to bind")?;
    axum::serve(listener, app)
        .await
        .context("Failed to serve")?;

    Ok(())
}
