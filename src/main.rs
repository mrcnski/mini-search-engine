use anyhow::{anyhow, Context};
use axum::{extract::Query, response::Html, routing::get, Router};
use serde::Deserialize;
use std::collections::HashMap;

mod consts;
mod routes

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO: Initial crawling/indexing.

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

