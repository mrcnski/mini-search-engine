use anyhow::{self, Context};
use spider::{
    configuration::{WaitForIdleNetwork, WaitForSelector},
    features::chrome_common::RequestInterceptConfiguration,
    page::Page,
    tokio,
    website::Website,
};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Instant,
};
use tokio::{sync::mpsc, task::JoinSet, time::Duration};

use crate::{consts, utils::log};

fn init(url: &str, page_limit: u32) -> anyhow::Result<Website> {
    let mut interception = RequestInterceptConfiguration::new(true);
    interception.block_javascript = true;

    Ok(Website::new(url)
        .with_limit(page_limit)
        .with_chrome_intercept(interception)
        .with_wait_for_idle_network(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(
            500,
        )))))
        .with_wait_for_idle_dom(Some(WaitForSelector::new(
            Some(Duration::from_millis(100)),
            "body".into(),
        )))
        .with_block_assets(true)
        // .with_wait_for_delay(Some(WaitForDelay::new(Some(Duration::from_millis(10000)))))
        .with_stealth(true)
        .with_return_page_links(true)
        .with_fingerprint(true)
        .with_respect_robots_txt(true)
        // .with_proxies(Some(vec!["http://localhost:8888".into()]))
        // .with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))
        .build()?)
}

pub async fn initial_crawl(indexer_tx: mpsc::Sender<Page>) -> anyhow::Result<()> {
    let start = Instant::now();

    // We assume one valid domain per line.
    let domains = tokio::fs::read_to_string(consts::DOMAINS_FILE).await?;
    // TODO: remove the `take`. Just want a small test set for now.
    let domains = domains.lines().take(20);

    // Have separate tasks for each domain. We'll process multiple domains in parallel, and
    // hopefully not get blocked or rate-limited from any target domain. This also follows the
    // `spider` examples (except they didn't use a JoinSet).
    let mut crawl_domain_tasks: JoinSet<()> = JoinSet::new();

    for domain in domains {
        println!("Crawling domain: {}", domain);

        let mut website = init(domain, consts::MAX_PAGES_PER_DOMAIN)?;
        let mut crawl_rx = website
            .subscribe(16)
            .context("Failed to subscribe to website crawler")?;
        let indexer_tx = Arc::new(indexer_tx.to_owned()); // Create owned value for the async task.
        let domain = Arc::new(domain.to_owned()); // Create owned value for the async task.

        crawl_domain_tasks.spawn(async move {
            let domain2 = domain.clone();

            // Spawn task that receives pages from the crawler.
            let recv_handle = tokio::task::spawn(async move {
                let page_count = Arc::new(AtomicU32::new(0));

                let mut crawl_page_tasks: JoinSet<()> = JoinSet::new();

                while let Ok(page) = crawl_rx.recv().await {
                    let page_count = page_count.clone();
                    let indexer_tx = indexer_tx.clone();
                    let domain = domain.clone();

                    // We use async and potentially-blocking methods, so spawn a task to avoid
                    // losing messages. See [`spider::website::Website::subscribe`].
                    crawl_page_tasks.spawn(async move {
                        // Provide some visual indication of crawl progress.
                        let cur_count = page_count
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| Some(x + 1))
                            .unwrap_or_else(|e| e);
                        if cur_count % consts::LOG_INTERVAL_PER_DOMAIN == 0 {
                            log(&format!("{domain}: crawled {cur_count} pages...\n")).await;
                        }

                        // Send page to indexer task.
                        if let Err(e) = indexer_tx.send(page).await {
                            log(&format!("ERROR: index receiver dropped: {e}")).await;
                        }
                    });

                    // Limit the number of tasks per domain.
                    while crawl_page_tasks.len() > 16 {
                        if let Some(Err(e)) = crawl_page_tasks.join_next().await {
                            log(&format!("WARNING: could not crawl: {e}")).await;
                        }
                    }
                }

                // Log any remaining tasks that returned an error.
                while let Some(result) = crawl_page_tasks.join_next().await {
                    if let Err(e) = result {
                        log(&format!("WARNING: could not crawl: {e}")).await;
                    }
                }
            });

            // Crawl, sending pages to page receiver, and unsubscribe when done.
            website.crawl().await;
            website.unsubscribe();

            if let Err(e) = recv_handle.await {
                log(&format!("WARNING: could not crawl: {e}")).await;
            }
            log(&format!("{domain2}: finished crawling!\n")).await;
        });
    }

    // Wait for all domain crawlers to finish.
    crawl_domain_tasks.join_all().await;

    let duration = start.elapsed();
    println!();
    println!("Finished crawling in {:?}", duration);

    Ok(())
}
