use anyhow::{anyhow, Context};
use axum::{routing::get, Router};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;

mod consts;
mod crawler;
mod routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial crawling/indexing.
    initial_crawl_and_index().await.context("Failed to crawl and index")?;

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

    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    let domains = tokio::fs::read_to_string(consts::DOMAINS_FILE).await?;
    // TODO: remove the `take`. Just want a small test set for now.
    let domains = domains.lines().take(2);

    // Create index directory if it doesn't exist
    let index_path = PathBuf::from(consts::SEARCH_INDEX_DIR);
    tokio::fs::create_dir_all(&index_path).await?;

    // let indexer = indexer::Indexer::new(&index_path)?;

    for domain in domains {
        println!("Crawling domain: {}", domain);

        let mut website = crawler::init_crawler(domain, MAX_PAGES_PER_DOMAIN)?;
        let mut rx2 = website.subscribe(16).unwrap();
        let stdout = stdout.clone();

        tokio::join!(
            async move {
                website.crawl().await;
                website.unsubscribe();
            },
            async move {
                while let Ok(page) = rx2.recv().await {
                    let _ = stdout.lock()
                        .expect("failed to access poisoned mutex (another thread panicked while holding the mutex)")
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
                        .await;
                }
            }
        );

        // for page_url in pages {
        //     // For now, we're just indexing the URLs
        //     // In a real implementation, we'd fetch and parse the content
        //     indexer.add_page(&page_url, &page_url)?;
        //     println!("Indexed: {}", page_url);
        // }
    }

    Ok(())
}
