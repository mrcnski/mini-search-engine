use axum::{extract::Query, response::Html, Extension};
use std::{collections::HashMap, time::Instant};
use tera::Context;

use super::ServerState;

pub async fn index_handler(
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

    let html = match super::TEMPLATES.render("index.html", &context) {
        Ok(html) => html,
        Err(e) => {
            eprintln!("Template error: {e}");

            let mut context = Context::new();
            context.insert("title", &config.name);
            context.insert("error", "An internal error occurred");

            super::TEMPLATES
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
    use axum::{body, http::Request, Router};
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::{
        config::{Config, ServerConfig},
        indexer::Indexer,
    };

    async fn with_app<F, T>(test_name: &str, f: F)
    where
        F: FnOnce(Router, ServerConfig) -> T,
        T: std::future::Future<Output = anyhow::Result<()>>,
    {
        let config = Config::load_test(test_name);
        let indexer = Arc::new(Indexer::new(&config.indexer).await.unwrap());
        let app = super::super::create_router(indexer.clone(), &config.server);

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
                    body::to_bytes(response.into_body(), 10_000).await?.to_vec(),
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
                    body::to_bytes(response.into_body(), 10_000).await?.to_vec(),
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
                    body::to_bytes(response.into_body(), 10_000).await?.to_vec(),
                )?;
                assert!(body.contains("Query too long")); // Should show error message

                Ok(())
            }
        })
        .await;
    }
}
