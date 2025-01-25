use anyhow;
use spider::configuration::WaitForSelector;
use spider::tokio;
use spider::website::Website;
use spider::{
    configuration::WaitForIdleNetwork, features::chrome_common::RequestInterceptConfiguration,
};
use tokio::time::Duration;

pub fn init(url: &str, page_limit: u32) -> anyhow::Result<Website> {
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
