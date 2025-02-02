use axum::{extract::Query, response::Html, routing::get, Extension, Router};
use std::{collections::HashMap, str::FromStr, sync::Arc, time::Instant};
use tera::{Context, Tera};

use crate::{CONFIG, indexer::Indexer};

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
        .layer(Extension(indexer))
}

pub async fn stats_handler(Extension(index): Extension<Arc<Indexer>>) -> Html<String> {
    let mut context = Context::new();
    context.insert("title", &CONFIG.server.name);

    match index.get_domain_stats() {
        Ok(stats) => {
            let total_pages: u64 = stats.iter().map(|s| s.page_count).sum();
            let total_size: u64 = stats
                .iter()
                .map(|s| {
                    bytesize::ByteSize::from_str(&s.total_size)
                        .map(|size| size.as_u64())
                        .unwrap_or(0)
                })
                .sum();

            context.insert("stats", &stats);
            context.insert("total_pages", &total_pages);
            context.insert(
                "total_size",
                &humansize::format_size(total_size, humansize::DECIMAL),
            );
        }
        Err(e) => {
            eprintln!("ERROR: Failed to get domain stats: {e}");
            context.insert("error", "Failed to get domain statistics");
        }
    }

    Html(
        TEMPLATES
            .render("stats.html", &context)
            .unwrap_or_else(|e| {
                eprintln!("Template error: {e}");
                "Template error".to_string()
            }),
    )
}

pub async fn index_handler(
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
                context.insert("error", "An error occurred while searching");
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
