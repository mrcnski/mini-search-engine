use anyhow::Context;
use axum::{routing::get, Router};
use std::sync::atomic::{AtomicU32, Ordering};
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
    const MAX_PAGES_PER_DOMAIN: u32 = 50;
    const LOG_INTERVAL_PER_DOMAIN: u32 = 10;

    // We assume one valid domain per line.
    let domains = tokio::fs::read_to_string(consts::DOMAINS_FILE).await?;
    // TODO: remove the `take`. Just want a small test set for now.
    let domains = domains.lines().take(2);

    let indexer = indexer::Indexer::new(consts::SEARCH_INDEX_DIR).await?;

    // Have separate tasks for each domain. We'll process multiple domains in parallel, and
    // hopefully not get blocked or rate-limited from any target domain. This also follows the
    // `spider` examples (except they didn't use a JoinSet).
    let mut tasks_set: JoinSet<anyhow::Result<()>> = JoinSet::new();

    for domain in domains {
        println!("Crawling domain: {}", domain);

        let mut website = crawler::init(domain, MAX_PAGES_PER_DOMAIN)?;
        let domain = domain.to_owned(); // Create owned value for the async task.
        let mut rx2 = website
            .subscribe(16)
            .context("Failed to subscribe to website crawler")?;
        let mut stdout = tokio::io::stdout();
        let page_count = AtomicU32::new(0);

        tasks_set.spawn(async move {
            let join_handle = tokio::task::spawn(async move {
                while let Ok(page) = rx2.recv().await {
                    let url = page.get_url();
                    let html = page.get_html();

                    // Provide some visual indication of crawl progress.
                    let cur_count = page_count
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| Some(x + 1))
                        .unwrap_or_else(|e| e);
                    if cur_count % LOG_INTERVAL_PER_DOMAIN == 0 {
                        stdout
                            .write_all(format!("{domain}: crawled {cur_count} pages\n").as_bytes())
                            .await?;
                        stdout.flush().await?;
                    }
                }

                Ok(())
            });

            // TODO: Use `scrape` to get the HTML documents for the website?
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
