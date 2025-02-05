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

use crate::{config::ServerConfig, indexer::Indexer};
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use std::future::Future;
    use tower::ServiceExt;

    use crate::Config;

    async fn with_app<F, T>(test_name: &str, f: F)
    where
        F: FnOnce(Router, ServerConfig) -> T,
        T: Future<Output = anyhow::Result<()>>,
    {
        let config = Config::load_test(test_name);
        let indexer = Arc::new(Indexer::new(&config.indexer).await.unwrap());
        let app = create_router(indexer.clone(), &config.server);

        f(app, config.server).await.unwrap();

        // Clean up after test.
        indexer.delete().await.unwrap();
    }

    #[tokio::test]
    async fn test_index_handler_no_query() {
        with_app("test_index_handler_no_query", |app, config| {
            async move {
                let response = app
                    .oneshot(Request::builder().uri("/").body("".to_string()).unwrap())
                    .await?;

                assert_eq!(response.status(), 200);
                let body = String::from_utf8(
                    axum::body::to_bytes(response.into_body(), 10_000)
                        .await?
                        .to_vec(),
                )?;
                assert!(body.contains(&config.name));
                assert!(!body.contains("results")); // No results section when no query

                Ok(())
            }
        })
        .await;
    }

    #[tokio::test]
    async fn test_index_handler_with_query() {
        with_app("test_index_handler_with_query", |app, config| {
            async move {
                let response = app
                    .oneshot(Request::builder().uri("/?q=test").body("".to_string())?)
                    .await?;

                assert_eq!(response.status(), 200);
                let body = String::from_utf8(
                    axum::body::to_bytes(response.into_body(), 10_000)
                        .await?
                        .to_vec(),
                )?;
                assert!(body.contains(&config.name));
                assert!(body.contains("query")); // Query should be shown

                Ok(())
            }
        })
        .await;
    }

    #[tokio::test]
    async fn test_index_handler_long_query() {
        with_app("test_index_handler_long_query", |app, _config| {
            async move {
                let long_query = "x".repeat(300);
                let response = app
                    .oneshot(
                        Request::builder()
                            .uri(&format!("/?q={}", long_query))
                            .body("".to_string())?,
                    )
                    .await?;

                assert_eq!(response.status(), 200);
                let body = String::from_utf8(
                    axum::body::to_bytes(response.into_body(), 10_000)
                        .await?
                        .to_vec(),
                )?;
                assert!(body.contains("Query too long")); // Should show error message

                Ok(())
            }
        })
        .await;
    }
}
