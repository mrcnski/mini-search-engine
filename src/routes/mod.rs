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

use crate::{indexer::Indexer, CONFIG};
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

pub fn create_router(indexer: Arc<Indexer>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/stats", get(stats_handler))
        .nest_service("/assets", get_service(ServeDir::new("assets")))
        .layer(Extension(indexer))
}

async fn index_handler(
    Query(params): Query<HashMap<String, String>>,
    Extension(index): Extension<Arc<Indexer>>,
) -> Html<String> {
    let mut context = Context::new();
    context.insert("title", &CONFIG.server.name);

    let query = params
        .iter()
        .filter_map(|(k, v)| if k == "q" { Some(v.clone()) } else { None })
        .collect::<Vec<_>>()
        .join("");

    if !query.is_empty() {
        context.insert("query", &query);

        let start = Instant::now();
        let search_result = index.search(&query, CONFIG.indexer.results_per_query);
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

    Html(
        TEMPLATES
            .render("index.html", &context)
            .unwrap_or_else(|e| {
                eprintln!("Template error: {e}");
                "Template error".to_string()
            }),
    )
}
