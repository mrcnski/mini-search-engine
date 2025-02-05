mod tech_terms;

use anyhow::Context;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use spider::page::Page;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tantivy::{
    collector::TopDocs,
    doc,
    query::{Query, QueryParser},
    schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, STORED},
    snippet::SnippetGenerator,
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
};
use tokio::sync::mpsc;

use crate::config::IndexerConfig;
use tech_terms::*;

pub struct Indexer {
    #[allow(dead_code)]
    index: Index,
    index_writer: Arc<RwLock<IndexWriter>>,
    schema: Schema,
    reader: Arc<RwLock<IndexReader>>,
    query_parser: Arc<RwLock<QueryParser>>,
    stats_db: sled::Db,
    is_dirty: AtomicBool,
    config: IndexerConfig,
}

impl Indexer {
    pub async fn new(config: &IndexerConfig) -> anyhow::Result<Self> {
        let schema = Self::create_schema();
        let index = Self::create_index(&schema, &config.index_dir, config.new_index).await?;
        let reader = Self::create_reader(&index)?;
        let index_writer: Arc<RwLock<IndexWriter>> =
            Arc::new(RwLock::new(index.writer(50_000_000)?));
        let query_parser = Self::create_query_parser(&index, &schema)?;
        let stats_db = Self::create_stats_db(config.new_index, &config.db_dir).await?;

        Ok(Indexer {
            index,
            index_writer,
            schema,
            reader,
            query_parser,
            stats_db,
            is_dirty: AtomicBool::new(false),
            config: config.clone(),
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        let text_options_fast = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("en_stem")
                    .set_index_option(IndexRecordOption::WithFreqs),
            )
            .set_stored()
            .set_fast(Some("en_stem"));
        let text_options_body = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("en_stem")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        schema_builder.add_text_field("title", text_options_fast.clone());
        schema_builder.add_text_field("description", text_options_fast);
        schema_builder.add_text_field("body", text_options_body);
        schema_builder.add_text_field("url", STORED);
        schema_builder.add_text_field("domain", STORED | FAST);
        schema_builder.add_u64_field("size", STORED | FAST);

        schema_builder.build()
    }

    async fn create_index(
        schema: &Schema,
        index_path: &str,
        new_index: bool,
    ) -> anyhow::Result<Index> {
        if new_index {
            // Delete any existing index.
            let _ = tokio::fs::remove_dir_all(index_path).await;
        }

        // Create index directory if it doesn't exist
        tokio::fs::create_dir_all(index_path).await?;

        let index = if new_index {
            Index::create_in_dir(index_path, schema.clone())?
        } else {
            Index::open_in_dir(index_path)?
        };

        Ok(index)
    }

    fn create_reader(index: &Index) -> anyhow::Result<Arc<RwLock<IndexReader>>> {
        Ok(Arc::new(RwLock::new(
            index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into()?,
        )))
    }

    fn create_query_parser(
        index: &Index,
        schema: &Schema,
    ) -> anyhow::Result<Arc<RwLock<QueryParser>>> {
        let title_field = schema.get_field("title").unwrap();
        let description_field = schema.get_field("description").unwrap();
        let body_field = schema.get_field("body").unwrap();

        let mut query_parser =
            QueryParser::for_index(index, vec![title_field, body_field, description_field]);

        // Boost title and description fields for more relevant searches.
        query_parser.set_field_boost(title_field, 2.0);
        query_parser.set_field_boost(body_field, 1.0);
        query_parser.set_field_boost(description_field, 1.5);

        // Enable fuzzy search for more error tolerance for the user.
        // REMOVED: breaks snippet generation.
        // query_parser.set_field_fuzzy(title_field, false, 1, true);
        // query_parser.set_field_fuzzy(body_field, false, 1, true);
        // query_parser.set_field_fuzzy(description_field, false, 1, true);

        Ok(Arc::new(RwLock::new(query_parser)))
    }

    async fn create_stats_db(new_index: bool, db_dir: &str) -> anyhow::Result<sled::Db> {
        if new_index {
            let _ = tokio::fs::remove_dir_all(db_dir).await;
        }

        // Create directory if it doesn't exist.
        if let Some(dir) = std::path::Path::new(db_dir).parent() {
            tokio::fs::create_dir_all(dir).await?;
        }

        Ok(sled::open(db_dir)?)
    }

    #[allow(dead_code)]
    pub async fn delete(&self) -> anyhow::Result<()> {
        let index_path = &self.config.index_dir;
        let db_dir = &self.config.db_dir;

        // Delete the index directory and stats database.
        let _ = tokio::fs::remove_dir_all(index_path).await;
        let _ = tokio::fs::remove_dir_all(db_dir).await;

        Ok(())
    }

    pub fn add_page(&self, SearchPage { page, domain }: &SearchPage) -> anyhow::Result<()> {
        let html = page.get_html();
        let url = page.get_url();

        let document = Html::parse_document(&html);

        let title_selector = Selector::parse("title").unwrap();
        let description_selector = Selector::parse(r#"meta[name="description"]"#).unwrap();
        let body_selector = Selector::parse("body").unwrap();

        let title = document
            .select(&title_selector)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();
        let description = document
            .select(&description_selector)
            .next()
            .map(|el| el.value().attr("content").unwrap_or_default())
            .unwrap_or_default();
        let body = if let Some(body) = document.select(&body_selector).next() {
            extract_text(body)
        } else {
            String::new()
        };
        let size = u64::try_from(body.len())?;

        let title_field = self.schema.get_field("title").unwrap();
        let description_field = self.schema.get_field("description").unwrap();
        let body_field = self.schema.get_field("body").unwrap();
        let url_field = self.schema.get_field("url").unwrap();
        let domain_field = self.schema.get_field("domain").unwrap();
        let size_field = self.schema.get_field("size").unwrap();

        let index_writer_wlock = self.index_writer.write().unwrap();
        index_writer_wlock.add_document(doc!(
            title_field => title,
            description_field => description,
            body_field => body,
            url_field => url,
            domain_field => domain.clone(),
            size_field => size,
        ))?;

        self.is_dirty.store(true, Ordering::Relaxed);

        self.update_domain_stats(domain, url, size)?;

        Ok(())
    }

    pub fn search(&self, query_str: &str, num_docs: usize) -> anyhow::Result<Vec<SearchResult>> {
        const MAX_QUERY_LENGTH: usize = 256;

        if query_str.len() > MAX_QUERY_LENGTH {
            return Err(anyhow::anyhow!(
                "Query too long - maximum length is {} characters",
                MAX_QUERY_LENGTH
            ));
        }

        let reader = self.reader.read().unwrap();
        let searcher = reader.searcher();

        let schema = &self.schema;
        let body_field = schema.get_field("body").unwrap();

        let query = self.construct_query(query_str)?;

        // Collect top results.
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(num_docs))
            .context("Could not execute search")?;

        // Display results.
        //
        // NOTE: We parallelize this because snippet generation can be expensive. We use threads
        // instead of tasks because of a strange compiler error. (Snippet generation is blocking,
        // anyway.)
        let mut threads = vec![];
        for (_score, doc_address) in top_docs {
            let (snippet_generator, retrieved_doc) = {
                let retrieved_doc: TantivyDocument = searcher.doc(doc_address).unwrap();

                // Create a SnippetGenerator
                let snippet_generator =
                    SnippetGenerator::create(&searcher, &*query, body_field).unwrap();

                (snippet_generator, retrieved_doc)
            };

            let title_field = schema.get_field("title").unwrap();
            let url_field = schema.get_field("url").unwrap();

            threads.push(std::thread::spawn(move || {
                let title = retrieved_doc
                    .get_first(title_field)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string();
                let url = retrieved_doc
                    .get_first(url_field)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string();

                let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);
                let snippet = snippet.to_html();

                SearchResult {
                    title,
                    url,
                    snippet,
                }
            }));
        }

        // TODO: add some timeout in case of a stalled thread.
        let results = threads
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        Ok(results)
    }

    /// Constructs a [`Query`] from the user input. We add a boost to certain tech terms to provide
    /// more relevant results.
    fn construct_query(&self, query_str: &str) -> anyhow::Result<Box<dyn Query>> {
        let query_parser = self.query_parser.read().unwrap();

        // For better performance, remove semicolons from the query before passing it to tantivy.
        let query_str = query_str.replace(";", " ");

        let boosted_query = boost_tech_terms(&query_str, self.config.tech_term_boost);

        // Parse the user query on a best-effort basis, ignoring any errors.
        let (query, _ignored_errors) = query_parser.parse_query_lenient(&boosted_query);

        Ok(query)
    }

    fn update_domain_stats(&self, domain: &str, url: &str, size: u64) -> anyhow::Result<()> {
        let stats_key = format!("domain:{domain}");
        let current_stats = self.stats_db.get(&stats_key)?.unwrap_or_default();
        let mut stats: RawDomainStats =
            bincode::deserialize(&current_stats).unwrap_or_else(|_| Default::default());

        stats.page_count += 1;
        stats.total_size += size;

        // Update min size and URL
        if size < stats.min_size {
            stats.min_size = size;
            stats.min_url = url.to_string();
        }
        // Update max size and URL
        if size > stats.max_size {
            stats.max_size = size;
            stats.max_url = url.to_string();
        }
        self.stats_db
            .insert(stats_key, bincode::serialize(&stats)?)?;

        Ok(())
    }

    pub fn get_domain_stats(&self) -> anyhow::Result<Vec<DomainStats>> {
        let mut stats = Vec::new();

        for item in self.stats_db.scan_prefix("domain:") {
            let (key, value) = item?;
            let domain = String::from_utf8(key.as_ref()[7..].to_vec())?;
            let raw_stats: RawDomainStats = bincode::deserialize(&value)?;

            stats.push(DomainStats {
                domain,
                page_count: raw_stats.page_count,
                total_size: humansize::format_size(raw_stats.total_size, humansize::DECIMAL),
                min_page_size: humansize::format_size(raw_stats.min_size, humansize::DECIMAL),
                max_page_size: humansize::format_size(raw_stats.max_size, humansize::DECIMAL),
                min_page_url: raw_stats.min_url,
                max_page_url: raw_stats.max_url,
            });
        }

        stats.sort_by(|a, b| a.domain.cmp(&b.domain));
        Ok(stats)
    }
}

fn extract_text(element: ElementRef) -> String {
    const IGNORED_ELEMENTS: &[&str] = &["script"];

    let mut text = String::new();

    for child in element.children() {
        match child.value() {
            // If the child is an element, check if it's a <script>
            Node::Element(e) => {
                if !IGNORED_ELEMENTS.contains(&e.name()) {
                    if let Some(el_ref) = ElementRef::wrap(child) {
                        text.push_str(&extract_text(el_ref));
                    }
                }
            }
            // If the child is a text node, append its content
            Node::Text(t) => {
                let t = t.trim();
                if !t.is_empty() {
                    text.push_str(t);
                    text.push(' '); // Add a space between text nodes
                }
            }
            _ => {}
        }
    }

    text
}

/// Splits a query string into terms, preserving quoted phrases
fn split_query_terms(query_str: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current_term = String::new();
    let mut in_quotes = false;

    for c in query_str.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        }

        match c {
            ' ' if !in_quotes => {
                // Space outside quotes - add current term if non-empty
                if !current_term.is_empty() {
                    terms.push(current_term.clone());
                    current_term.clear();
                }
            }
            _ => current_term.push(c),
        }
    }
    // Add final term if non-empty
    if !current_term.is_empty() {
        terms.push(current_term);
    }
    terms
}

/// Applies boosting to tech terms in the query
fn boost_tech_terms(query_str: &str, tech_term_boost: f32) -> String {
    let terms = split_query_terms(query_str);

    terms
        .into_iter()
        .map(|term| {
            if !term.contains('"')
                && TECH_TERMS_TO_BOOST
                    .iter()
                    .any(|tech| tech.eq_ignore_ascii_case(&term))
            {
                format!("{}^{}", term, tech_term_boost)
            } else {
                term
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Serialize, Deserialize)]
struct RawDomainStats {
    page_count: u64,
    total_size: u64,
    min_size: u64,
    max_size: u64,
    min_url: String,
    max_url: String,
}

impl Default for RawDomainStats {
    fn default() -> Self {
        Self {
            page_count: Default::default(),
            total_size: Default::default(),
            min_size: u64::MAX,
            max_size: Default::default(),
            min_url: Default::default(),
            max_url: Default::default(),
        }
    }
}

/// Domain stats with human-readable values.
#[derive(Serialize)]
pub struct DomainStats {
    pub domain: String,
    pub page_count: u64,
    pub total_size: String,
    pub min_page_size: String,
    pub max_page_size: String,
    pub min_page_url: String,
    pub max_page_url: String,
}

/// The result of a web search.
#[derive(Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    /// A relevant snippet from the page.
    pub snippet: String,
}

pub struct SearchPage {
    pub page: Page,
    pub domain: String,
}

pub async fn start(
    config: &IndexerConfig,
) -> anyhow::Result<(Arc<Indexer>, mpsc::Sender<SearchPage>)> {
    let indexer = Arc::new(Indexer::new(config).await?);
    let add_page_indexer = indexer.clone();
    let commit_indexer = indexer.clone();

    let (tx, mut rx) = mpsc::channel(1000);

    tokio::task::spawn(async move {
        while let Some(page) = rx.recv().await {
            if let Err(e) = add_page_indexer.add_page(&page) {
                let url = page.page.get_url();
                eprintln!("ERROR: could not index page '{url}': {e}");
            }
        }
    });

    // Periodically commit.
    // NOTE: Committing can block, and is also non-async, so we use a dedicated thread.
    let commit_interval_ms = config.commit_interval_ms;
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(commit_interval_ms));

        // Skip if there's nothing to commit.
        if !commit_indexer.is_dirty.load(Ordering::Relaxed) {
            continue;
        }

        let mut index_writer_wlock = commit_indexer.index_writer.write().unwrap();
        if let Err(e) = index_writer_wlock.commit() {
            eprintln!("ERROR: could not commit index: {e}");
        } else {
            commit_indexer.is_dirty.store(false, Ordering::Relaxed);
        }
    });

    Ok((indexer, tx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::Html;

    const TECH_TERM_BOOST: f32 = 1.5;

    #[test]
    fn test_split_query_terms() {
        // Test basic splitting
        assert_eq!(split_query_terms("hello world"), vec!["hello", "world"]);

        // Test quoted phrases
        assert_eq!(
            split_query_terms("hello \"world peace\""),
            vec!["hello", "\"world peace\""]
        );

        // Test multiple quoted phrases
        assert_eq!(
            split_query_terms("\"hello there\" world \"peace now\""),
            vec!["\"hello there\"", "world", "\"peace now\""]
        );

        // Test empty quotes
        assert_eq!(
            split_query_terms("hello \"\" world"),
            vec!["hello", "\"\"", "world"]
        );

        // Test unclosed quotes
        assert_eq!(split_query_terms("hello \"world"), vec!["hello", "\"world"]);
    }

    #[test]
    fn test_boost_tech_terms() {
        // Test basic term boost
        assert_eq!(
            boost_tech_terms("rust programming", TECH_TERM_BOOST),
            format!("rust^{} programming", TECH_TERM_BOOST)
        );

        // Test quoted phrase (should not boost)
        assert_eq!(
            boost_tech_terms("\"rust programming\"", TECH_TERM_BOOST),
            "\"rust programming\""
        );

        // Test mixed terms
        assert_eq!(
            boost_tech_terms(
                "learning rust \"in python\" with javascript",
                TECH_TERM_BOOST
            ),
            format!(
                "learning rust^{} \"in python\" with javascript^{}",
                TECH_TERM_BOOST, TECH_TERM_BOOST
            )
        );

        // Test case insensitivity
        assert_eq!(
            boost_tech_terms("RUST Python", TECH_TERM_BOOST),
            format!("RUST^{} Python^{}", TECH_TERM_BOOST, TECH_TERM_BOOST)
        );

        // Test non-tech terms
        assert_eq!(
            boost_tech_terms("hello world", TECH_TERM_BOOST),
            "hello world"
        );
    }

    #[test]
    fn test_extract_text() {
        // Test case 1: Simple text without script
        let html = r#"<body>Hello world</body>"#;
        let document = Html::parse_document(html);
        let body = document
            .select(&Selector::parse("body").unwrap())
            .next()
            .unwrap();
        assert_eq!(extract_text(body), "Hello world ");

        // Test case 2: Text with script at root level
        let html = r#"<body>Hello <script>alert('hidden');</script>world</body>"#;
        let document = Html::parse_document(html);
        let body = document
            .select(&Selector::parse("body").unwrap())
            .next()
            .unwrap();
        assert_eq!(extract_text(body), "Hello world ");

        // Test case 3: Text with nested script
        let html =
            r#"<body>Hello <div>nested <script>alert('hidden');</script>text</div> world</body>"#;
        let document = Html::parse_document(html);
        let body = document
            .select(&Selector::parse("body").unwrap())
            .next()
            .unwrap();
        assert_eq!(extract_text(body), "Hello nested text world ");

        // Test case 4: Multiple scripts and nested elements
        let html = r#"<body>
            <h1>Title</h1>
            <script>var x = 1;</script>
            <div>
                Content
                <p>Paragraph <script>console.log('hidden');</script> text</p>
            </div>
            <script>var y = 2;</script>
        </body>"#;
        let document = Html::parse_document(html);
        let body = document
            .select(&Selector::parse("body").unwrap())
            .next()
            .unwrap();
        assert_eq!(extract_text(body), "Title Content Paragraph text ");
    }
}
