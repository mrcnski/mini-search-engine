use axum::{response::Html, Extension};
use std::str::FromStr;
use tera::Context;

use super::{ServerState, TEMPLATES};

pub async fn stats_handler(
    Extension(ServerState { indexer, config }): Extension<ServerState>,
) -> Html<String> {
    let mut context = Context::new();
    context.insert("title", &config.name);

    match indexer.get_domain_stats() {
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
