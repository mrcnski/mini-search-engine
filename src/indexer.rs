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
    schema::{Schema, Value, FAST, STORED, TEXT},
    tokenizer, Index, IndexReader, IndexWriter, ReloadPolicy, SnippetGenerator, TantivyDocument,
};
use tokio::sync::mpsc;

use crate::consts;

/// Boost factor applied to tech terms in queries
const TECH_TERM_BOOST: f32 = 1.5;

/// Tech terms to boost within queries. Generated from `domains` using AI.
const TECH_TERMS_TO_BOOST: &[&str] = &[
    "actix",
    "angular",
    "ansible",
    "astro",
    "aws",
    "azure",
    "bash",
    "c",
    "c-lang",
    "clang",
    "c++",
    "cpp",
    "c++-lang",
    "cpplang",
    "clojure",
    "clojurescript",
    "coffee",
    "coffeescript",
    "crystal",
    "crystal-lang",
    "css",
    "dart",
    "dart-lang",
    "dartlang",
    "deno",
    "django",
    "docker",
    "dotnet",
    "elixir",
    "elixir-lang",
    "elixirlang",
    "ember",
    "erlang",
    "erlang-lang",
    "erlanglang",
    "fastapi",
    "flask",
    "flutter",
    "gatsby",
    "git",
    "github",
    "gitlab",
    "go",
    "golang",
    "gradle",
    "graphql",
    "groovy",
    "haskell",
    "html",
    "java",
    "java-lang",
    "javalang",
    "javascript",
    "jenkins",
    "jquery",
    "js",
    "json",
    "jupyter",
    "kafka",
    "kotlin",
    "kotlin-lang",
    "kubernetes",
    "laravel",
    "linux",
    "lisp",
    "lua",
    "lua-lang",
    "lualang",
    "maven",
    "mongodb",
    "mysql",
    "nextjs",
    "nginx",
    "nim",
    "nim-lang",
    "nimlang",
    "nodejs",
    "nosql",
    "npm",
    "nuxt",
    "ocaml",
    "perl",
    "perl-lang",
    "php",
    "php-lang",
    "postgres",
    "postgresql",
    "python",
    "python-lang",
    "pythonlang",
    "r",
    "rails",
    "react",
    "reactjs",
    "redis",
    "redux",
    "ruby",
    "ruby-lang",
    "rubylang",
    "rust",
    "rust-lang",
    "rustlang",
    "scala",
    "scala-lang",
    "scalalang",
    "scheme",
    "shell",
    "shell-lang",
    "shelllang",
    "spring",
    "sql",
    "sqlite",
    "svelte",
    "swift",
    "swift-lang",
    "swiftlang",
    "terraform",
    "typescript",
    "ts",
    "vim",
    "vue",
    "vuejs",
    "webpack",
    "xml",
    "yaml",
    "zig",
];

pub struct Indexer {
    #[allow(dead_code)]
    index: Index,
    index_writer: Arc<RwLock<IndexWriter>>,
    schema: Schema,
    reader: Arc<RwLock<IndexReader>>,
    query_parser: Arc<RwLock<QueryParser>>,
    stats_db: sled::Db,
    is_dirty: AtomicBool,
}

impl Indexer {
    pub async fn new(index_path: &str, new_index: bool) -> anyhow::Result<Self> {
        let schema = Self::create_schema();
        let index = Self::create_index(&schema, index_path, new_index).await?;
        let reader = Self::create_reader(&index)?;
        let index_writer: Arc<RwLock<IndexWriter>> =
            Arc::new(RwLock::new(index.writer(50_000_000)?));
        let query_parser = Self::create_query_parser(&index, &schema)?;
        let stats_db = Self::create_stats_db(new_index)?;

        Ok(Indexer {
            index,
            index_writer,
            schema,
            reader,
            query_parser,
            stats_db,
            is_dirty: AtomicBool::new(false),
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED | FAST);
        schema_builder.add_text_field("description", TEXT | STORED | FAST);
        schema_builder.add_text_field("body", TEXT | STORED);
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
            let _ = tokio::fs::remove_dir_all(index_path).await?;
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

    fn create_stats_db(new_index: bool) -> anyhow::Result<sled::Db> {
        if new_index {
            let _ = std::fs::remove_dir_all(consts::DB_NAME);
        }

        Ok(sled::open(consts::DB_NAME)?)
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
        let reader = self.reader.read().unwrap();
        let searcher = reader.searcher();

        let schema = &self.schema;
        let title_field = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let url_field = self.schema.get_field("url").unwrap();

        let query = self.construct_query(query_str)?;

        // Collect top results.
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(num_docs))
            .context("Could not execute search")?;

        // Create a SnippetGenerator
        let snippet_generator = SnippetGenerator::create(&searcher, &*query, body_field)?;

        // Display results.
        let results = top_docs
            .into_iter()
            .map(|(_score, doc_address)| {
                let retrieved_doc: TantivyDocument = searcher.doc(doc_address).unwrap();

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
            })
            .collect();

        Ok(results)
    }

    /// Constructs a [`Query`] from the user input. We add a boost to certain tech terms to provide
    /// more relevant results.
    fn construct_query(&self, query_str: &str) -> anyhow::Result<Box<dyn Query>> {
        let query_parser = self.query_parser.read().unwrap();
        let boosted_query = boost_tech_terms(query_str);

        let query = query_parser
            .parse_query(&boosted_query)
            .context("Could not parse query")?;

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
fn boost_tech_terms(query_str: &str) -> String {
    let terms = split_query_terms(query_str);

    terms
        .into_iter()
        .map(|term| {
            if !term.contains('"')
                && TECH_TERMS_TO_BOOST
                    .iter()
                    .any(|tech| tech.eq_ignore_ascii_case(&term))
            {
                format!("{}^{}", term, TECH_TERM_BOOST)
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

pub async fn start(new_index: bool) -> anyhow::Result<(Arc<Indexer>, mpsc::Sender<SearchPage>)> {
    let indexer = Arc::new(Indexer::new(consts::SEARCH_INDEX_DIR, new_index).await?);
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
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));

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
            boost_tech_terms("rust programming"),
            format!("rust^{} programming", TECH_TERM_BOOST)
        );

        // Test quoted phrase (should not boost)
        assert_eq!(
            boost_tech_terms("\"rust programming\""),
            "\"rust programming\""
        );

        // Test mixed terms
        assert_eq!(
            boost_tech_terms("learning rust \"in python\" with javascript"),
            format!(
                "learning rust^{} \"in python\" with javascript^{}",
                TECH_TERM_BOOST, TECH_TERM_BOOST
            )
        );

        // Test case insensitivity
        assert_eq!(
            boost_tech_terms("RUST Python"),
            format!("RUST^{} Python^{}", TECH_TERM_BOOST, TECH_TERM_BOOST)
        );

        // Test non-tech terms
        assert_eq!(boost_tech_terms("hello world"), "hello world");
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
