use anyhow::{self, Context};
use scraper::{Html, Selector};
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
use tokio::{
    sync::{broadcast, mpsc},
    task::{JoinHandle, JoinSet},
    time::Duration,
};
use url::{ParseError, Url};

use crate::{consts, indexer::SearchPage};

struct DomainCrawler {
    website: Website,
    domain: String,
}

impl DomainCrawler {
    fn new(domain: &str, page_limit: u32) -> anyhow::Result<Self> {
        // Some chrome settings.
        let mut interception = RequestInterceptConfiguration::new(true);
        interception.block_javascript = true;

        let website = Website::new(domain)
            .with_limit(page_limit)
            // NOTE: Accept invalid certs as we prioritize relevance over security.
            .with_danger_accept_invalid_certs(true)
            .with_depth(0) // No max crawl depth. Use page limit only.
            .with_block_assets(true)
            .with_return_page_links(true) // TODO: set to false?
            .with_respect_robots_txt(true)
            .with_normalize(true)
            // Some chrome settings.
            .with_chrome_intercept(interception)
            .with_fingerprint(true)
            .with_stealth(true)
            // .with_wait_for_delay(Some(WaitForDelay::new(Some(Duration::from_millis(10000)))))
            .with_wait_for_idle_network(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(
                500,
            )))))
            .with_wait_for_idle_dom(Some(WaitForSelector::new(
                Some(Duration::from_millis(200)),
                "body".into(),
            )))
            // .with_proxies(Some(vec!["http://localhost:8888".into()]))
            // .with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))
            .build()?;

        // // Follow meta refresh redirects.
        // fn on_should_crawl_callback(page: &mut Page) -> bool {
        //     let redirect = get_meta_redirect_url(&page.get_html(), page.get_url());

        //     if let Some(redirect) = redirect {
        //         page.final_redirect_destination = Some(redirect);
        //     }

        //     true
        // }
        // website.on_should_crawl_callback = Some(on_should_crawl_callback);

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

        Ok(recv_handle.await??)
    }

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

                    Self::handle_page(page, indexer_tx, page_count, &domain.as_ref())
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

fn get_meta_redirect_url(html: &str, url: &str) -> Option<String> {
    let document = Html::parse_document(html);

    let refresh_selector = Selector::parse(r#"meta[http-equiv="refresh"]"#).unwrap();

    // Get the first meta tag that matches.
    let el = document.select(&refresh_selector).next()?;

    let content = el.value().attr("content")?;
    let url_part = content.split(";").nth(1)?.trim();
    let path = if url_part.starts_with("url=") {
        url_part.trim_start_matches("url=").trim()
    } else if url_part.starts_with("URL=") {
        url_part.trim_start_matches("URL=").trim()
    } else {
        return None;
    };
    let path = path.trim_matches(|c| c == '\'' || c == '"');

    resolve_url(url, path).ok()
}

fn resolve_url(base: &str, path: &str) -> Result<String, ParseError> {
    let base_url = Url::parse(base)?;
    let resolved_url = base_url.join(path)?;
    Ok(resolved_url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_meta_redirect_url() {
        // Basic redirect
        let html = r#"<html><head><meta http-equiv="refresh" content="0; url=https://example.com"></head></html>"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com"),
            Some("https://example.com/".to_string())
        );

        // Redirect with delay
        let html = r#"<html><head><meta http-equiv="refresh" content="5;url=https://example.com/page"></head></html>"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com"),
            Some("https://example.com/page".to_string())
        );

        // Relative URL redirect
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=/relative/path"></head></html>"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com/en"),
            Some("https://source.com/relative/path".to_string())
        );
        let html = r#"<html><head><meta http-equiv="refresh" content="0;url=relative/path"></head></html>"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com/en"),
            Some("https://source.com/relative/path".to_string())
        );

        // No redirect
        let html = r#"<html><head><title>No redirect here</title></head></html>"#;
        assert_eq!(get_meta_redirect_url(html, "https://source.com"), None);

        // Malformed content attribute
        let html = r#"<html><head><meta http-equiv="refresh" content="malformed"></head></html>"#;
        assert_eq!(get_meta_redirect_url(html, "https://source.com"), None);

        // Relative path without leading slash
        let html = r#"<meta http-equiv="refresh" content="0; url=en/latest/contents.html" />"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com"),
            Some("https://source.com/en/latest/contents.html".to_string())
        );

        // URL capitalized and with single quotes
        let html = r#"
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Strict//EN"
  "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="en">
<head>
<meta http-equiv="content-type" content="text/html;charset=utf-8" />
<title>Moved</title>
<meta http-equiv="refresh" content="0;URL='en/'" />
<script>window.ohcglobal || document.write('<script src="/en/dcommon/js/global.js">\x3C/script>')</script></head>
<body>
<p></p>
</body>
</html>
"#;
        assert_eq!(
            get_meta_redirect_url(html, "https://source.com"),
            Some("https://source.com/en/".to_string())
        );
    }
}
