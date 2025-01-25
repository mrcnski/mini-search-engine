use anyhow::Context;
use axum::{routing::get, Router};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinSet;

mod consts;
mod crawler;
mod indexer;
mod routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial crawling/indexing.
    initial_crawl_and_index()
        .await
        .context("Failed to crawl and index")?;

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

async fn initial_crawl_and_index() -> anyhow::Result<()> {
    const MAX_PAGES_PER_DOMAIN: u32 = 10;

    // We assume one valid domain per line.
    let domains = tokio::fs::read_to_string(consts::DOMAINS_FILE).await?;
    // TODO: remove the `take`. Just want a small test set for now.
    let domains = domains.lines().take(2);

    let indexer = indexer::Indexer::new(consts::SEARCH_INDEX_DIR).await?;

    // Have separate tasks for each domain. We'll process multiple domains in parallel, and
    // hopefully not get blocked or rate-limited from any target domain.
    let mut tasks_set: JoinSet<anyhow::Result<()>> = JoinSet::new();

    for domain in domains {
        println!("Crawling domain: {}", domain);

        let mut website = crawler::init(domain, MAX_PAGES_PER_DOMAIN)?;
        let mut rx2 = website
            .subscribe(16)
            .context("Failed to subscribe to website crawler")?;
        let mut stdout = tokio::io::stdout();

        tasks_set.spawn(async move {
            let join_handle = tokio::task::spawn(async move {
                while let Ok(page) = rx2.recv().await {
                    stdout
                        .write_all(
                            format!(
                                "- {} -- Bytes transferred {:?} -- HTML Size {:?} -- Links: {:?}\n",
                                page.get_url(),
                                page.bytes_transferred.unwrap_or_default(),
                                page.get_html_bytes_u8().len(),
                                match page.page_links {
                                    Some(ref l) => l.len(),
                                    _ => 0,
                                }
                            )
                            .as_bytes(),
                        )
                        .await?;
                    stdout.flush().await?;
                }

                Ok(())
            });

            // TODO: Use `scrape` to get the HTML documents for the website.
            website.crawl().await;
            website.unsubscribe();

            join_handle.await.unwrap()
        });
    }

    // Wait for all domain crawlers to finish.
    // TODO: Log any tasks that early-returned an error.
    let _ = tasks_set.join_all().await;

    Ok(())
}
