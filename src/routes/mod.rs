use axum::{
    routing::{get, get_service},
    Extension, Router,
};
use std::sync::Arc;
use tera::Tera;
use tower_http::services::ServeDir;

mod index;
mod stats;

use crate::{config::ServerConfig, indexer::Indexer};
use index::index_handler;
use stats::stats_handler;

lazy_static::lazy_static! {
    static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![]); // Disable autoescaping
        tera
    };
}

#[derive(Clone)]
struct ServerState {
    indexer: Arc<Indexer>,
    config: ServerConfig,
}

pub fn create_router(indexer: Arc<Indexer>, config: &ServerConfig) -> Router {
    let state = ServerState {
        indexer,
        config: config.clone(),
    };

    Router::new()
        .route("/", get(index_handler))
        .route("/stats", get(stats_handler))
        .nest_service("/assets", get_service(ServeDir::new("assets")))
        .layer(Extension(state))
}
