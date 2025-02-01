use anyhow::{self, Context};
use spider::{
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
use tokio::{
    sync::{broadcast, mpsc},
    task::{JoinHandle, JoinSet},
};

use crate::{consts, indexer::SearchPage};

struct DomainCrawler {
    website: Website,
    domain: String,
}

impl DomainCrawler {
    fn new(domain: &str, page_limit: u32) -> anyhow::Result<Self> {
        let website = Website::new(domain)
            .with_limit(page_limit)
            .with_depth(0) // No max crawl depth. Use page limit only.
            // NOTE: Accept invalid certs as we prioritize relevance over security.
            .with_danger_accept_invalid_certs(true)
            .with_block_assets(true)
            .with_respect_robots_txt(true)
            .with_normalize(true)
            .build()?;

        Ok(Self {
            website,
            domain: domain.to_string(),
        })
    }

    /// Crawl, sending pages to page receiver, and unsubscribe when done.
    async fn crawl_domain(&mut self, indexer_tx: mpsc::Sender<SearchPage>) -> anyhow::Result<()> {
        let crawl_rx = self
            .website
            .subscribe(16)
            .context("Failed to subscribe to website crawler")?;

        // Spawn task that receives pages from the crawler.
        let recv_handle = self.spawn_page_handler(crawl_rx, indexer_tx).await;

        self.website.crawl().await;
        self.website.unsubscribe();

        recv_handle.await?
    }

    /// Spawns the page handler which takes care of incoming pages from `website.crawl`. Once
    /// `website.crawl` signals that it has exhausted all pages, the returned future resolves.
    async fn spawn_page_handler(
        &self,
        mut crawl_rx: broadcast::Receiver<Page>,
        indexer_tx: mpsc::Sender<SearchPage>,
    ) -> JoinHandle<anyhow::Result<()>> {
        let domain = Arc::new(self.domain.to_owned()); // Create owned value for the async task.

        tokio::task::spawn(async move {
            let page_count = Arc::new(AtomicU32::new(0));

            let mut crawl_page_tasks: JoinSet<anyhow::Result<()>> = JoinSet::new();

            while let Ok(page) = crawl_rx.recv().await {
                let page_count = page_count.clone();
                let indexer_tx = indexer_tx.clone();
                let domain = domain.clone();

                // We use async and potentially-blocking methods, so spawn a task to avoid
                // losing messages. See [`spider::website::Website::subscribe`].
                crawl_page_tasks.spawn(async move {
                    let url = page.get_url().to_string();

                    Self::handle_page(page, indexer_tx, page_count, domain.as_ref())
                        .await
                        .with_context(|| format!("Failed to handle crawled page: {url}"))
                });

                // Limit the number of tasks per domain.
                while crawl_page_tasks.len() > 16 {
                    // We just checked the length, unwrap.
                    crawl_page_tasks.join_next().await.unwrap()??;
                }
            }

            // Log any remaining tasks that returned an error.
            while let Some(result) = crawl_page_tasks.join_next().await {
                result??;
            }

            Ok(())
        })
    }

    async fn handle_page(
        page: Page,
        indexer_tx: mpsc::Sender<SearchPage>,
        page_count: Arc<AtomicU32>,
        domain: &str,
    ) -> anyhow::Result<()> {
        // Provide some visual indication of crawl progress.
        let cur_count = page_count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| Some(x + 1))
            .unwrap_or_else(|e| e)
            + 1; // Add 1 since the previous value is returned.
        if cur_count % consts::LOG_INTERVAL_PER_DOMAIN == 0 {
            println!("{domain}: crawled {cur_count} pages...");
        }

        // Send page to indexer task.
        indexer_tx
            .send(SearchPage {
                page,
                domain: domain.to_string(),
            })
            .await
            .context("index receiver dropped")?;

        Ok(())
    }
}

pub async fn initial_crawl(indexer_tx: mpsc::Sender<SearchPage>) -> anyhow::Result<()> {
    let domains = get_domains_to_crawl().await?;

    let start = Instant::now();
    crawl_domains(domains, indexer_tx).await?;
    let duration = start.elapsed();

    println!();
    println!("Finished crawling in {:?}", duration);

    Ok(())
}

async fn crawl_domains(
    domains: Vec<String>,
    indexer_tx: mpsc::Sender<SearchPage>,
) -> anyhow::Result<()> {
    // Have separate tasks for each domain. We'll process multiple domains in parallel, and
    // hopefully not get blocked or rate-limited from any target domain. This also follows the
    // `spider` examples (except they didn't use a `JoinSet`).
    let mut crawl_domain_tasks: JoinSet<anyhow::Result<String>> = JoinSet::new();

    for domain in domains {
        println!("Crawling domain: {}", domain);

        let indexer_tx = indexer_tx.clone();
        crawl_domain_tasks.spawn(async move {
            let mut crawler = DomainCrawler::new(&domain, consts::MAX_PAGES_PER_DOMAIN)
                .with_context(|| format!("{domain}: Failed to create crawler"))?;
            crawler
                .crawl_domain(indexer_tx)
                .await
                .with_context(|| format!("{domain}: Failed to crawl domain"))?;

            Ok(domain)
        });
    }

    // Wait for all domain crawlers to finish.
    while let Some(result) = crawl_domain_tasks.join_next().await {
        match result? {
            Ok(domain) => println!("{domain}: finished crawling!"),
            Err(e) => eprintln!("ERROR: failed to crawl: {e}"),
        }
    }

    Ok(())
}

async fn get_domains_to_crawl() -> anyhow::Result<Vec<String>> {
    // We assume one valid domain per line.
    let domains = tokio::fs::read_to_string(consts::DOMAINS_FILE).await?;
    // TODO: remove the `take`. Just want a small test set for now.
    let domains = domains.lines().take(20);
    Ok(domains.map(|s| s.to_string()).collect())
}
