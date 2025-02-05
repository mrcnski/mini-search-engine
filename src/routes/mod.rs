use axum::{
    extract::Query,
    response::Html,
    routing::{get, get_service},
    Extension, Router,
};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tera::{Context, Tera};
use tower_http::services::ServeDir;

mod stats;

use crate::{indexer::Indexer, config::ServerConfig};
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

async fn index_handler(
    Query(params): Query<HashMap<String, String>>,
    Extension(ServerState { indexer, config }): Extension<ServerState>,
) -> Html<String> {
    let mut context = Context::new();
    context.insert("title", &config.name);

    let query = params
        .iter()
        .filter_map(|(k, v)| if k == "q" { Some(v.clone()) } else { None })
        .collect::<Vec<_>>()
        .join("");

    if !query.is_empty() {
        context.insert("query", &query);

        let start = Instant::now();
        let search_result = indexer.search(&query, config.results_per_query);
        let duration = start.elapsed();

        match search_result {
            Ok(results) => {
                context.insert("results", &results);
                context.insert("num_results", &results.len());
                context.insert("duration", &format!("{duration:?}"));
            }
            Err(e) => {
                eprintln!("ERROR: Search error for '{query}': {e}");
                let error_msg = if e.to_string().contains("Query too long") {
                    e.to_string()
                } else {
                    "An error occurred while searching".to_string()
                };
                context.insert("error", &error_msg);
            }
        }
    }

    let html = match TEMPLATES.render("index.html", &context) {
        Ok(html) => html,
        Err(e) => {
            eprintln!("Template error: {e}");

            let mut context = Context::new();
            context.insert("title", &config.name);
            // TODO: Call .user_error() on custom error instance.
            //       Have a separate .server_error() so that the server error doesn't accidentally leak.
            context.insert("error", "An internal error occurred");

            TEMPLATES
                .render("index.html", &context)
                .unwrap_or_else(|e| {
                    eprintln!("Critical template error: {e}");
                    "<h1>Internal Server Error</h1>".to_string()
                })
        }
    };
    Html(html)
}
}
