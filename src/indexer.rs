use anyhow::Context;
use scraper::{Html, Selector};
use spider::page::Page;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Schema, Value, FAST, STORED, TEXT},
    Index, IndexReader, IndexWriter, ReloadPolicy, Snippet, SnippetGenerator, TantivyDocument,
    tokenizer,
};
use tokio::sync::mpsc;

use crate::consts;

pub struct Indexer {
    #[allow(dead_code)]
    index: Index,
    index_writer: Arc<RwLock<IndexWriter>>,
    schema: Schema,
    reader: Arc<RwLock<IndexReader>>,
    query_parser: Arc<RwLock<QueryParser>>,
}

impl Indexer {
    pub async fn new(index_path: &str) -> anyhow::Result<Self> {
        let schema = Self::create_schema();
        let index = Self::create_index(&schema, index_path).await?;
        let reader = Self::create_reader(&index)?;

        let index_writer: Arc<RwLock<IndexWriter>> =
            Arc::new(RwLock::new(index.writer(50_000_000)?));

        let title_field = schema.get_field("title").unwrap();
        let description_field = schema.get_field("description").unwrap();
        let body_field = schema.get_field("body").unwrap();

        let mut query_parser =
            QueryParser::for_index(&index, vec![title_field, body_field, description_field]);
        query_parser.set_field_boost(title_field, 2.0);
        query_parser.set_field_boost(body_field, 1.0);
        query_parser.set_field_boost(description_field, 1.5);

        let query_parser = Arc::new(RwLock::new(query_parser));

        Ok(Indexer {
            index,
            index_writer,
            schema,
            reader,
            query_parser,
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED | FAST);
        schema_builder.add_text_field("description", TEXT | STORED | FAST);
        schema_builder.add_text_field("body", TEXT | STORED);
        schema_builder.add_text_field("url", STORED);
        schema_builder.build()
    }

    async fn create_index(schema: &Schema, index_path: &str) -> anyhow::Result<Index> {
        // Delete any existing index.
        // TODO: Remove this once we finalize the schema.
        let _ = tokio::fs::remove_dir_all(index_path).await?;

        // Create index directory if it doesn't exist
        tokio::fs::create_dir_all(index_path).await?;

        let mut index = Index::create_in_dir(index_path, schema.clone())?;

        let ff_tokenizer_manager = tokenizer::TokenizerManager::default();
        ff_tokenizer_manager.register(
            "raw",
            tokenizer::TextAnalyzer::builder(tokenizer::RawTokenizer::default())
                .filter(tokenizer::RemoveLongFilter::limit(255))
                .build(),
        );
        index.set_fast_field_tokenizers(ff_tokenizer_manager.clone());

        Ok(index)
    }

    fn create_reader(index: &Index) -> anyhow::Result<Arc<RwLock<IndexReader>>> {
        Ok(Arc::new(RwLock::new(
            index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                // .warmers(vec![Box::new(|searcher| {
                //     searcher
                //         .segment_readers()
                //         .iter()
                //         .for_each(|segment_reader| {
                //             let _ = segment_reader.fast_fields().str("title");
                //             let _ = segment_reader.fast_fields().str("description");
                //         });
                // })])
                .try_into()?,
        )))
    }

    pub fn add_page(&self, page: &Page) -> anyhow::Result<()> {
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
        let body = document
            .select(&body_selector)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        let title_field = self.schema.get_field("title").unwrap();
        let description_field = self.schema.get_field("description").unwrap();
        let body_field = self.schema.get_field("body").unwrap();
        let url_field = self.schema.get_field("url").unwrap();

        let index_writer_wlock = self.index_writer.write().unwrap();
        index_writer_wlock.add_document(doc!(
            title_field => title,
            description_field => description,
            body_field => body,
            url_field => url,
        ))?;

        Ok(())
    }

    pub fn search(&self, query_str: &str, num_docs: usize) -> anyhow::Result<Vec<SearchResult>> {
        let reader = self.reader.read().unwrap();
        let searcher = reader.searcher();

        let schema = &self.schema;
        let title_field = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let url_field = schema.get_field("url").unwrap();

        let query_parser = self.query_parser.read().unwrap();
        let query = query_parser
            .parse_query(query_str)
            .context("Could not parse query")?;

        // Collect top results.
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(num_docs))
            .context("Could not execute search")?;

        // Create a SnippetGenerator
        let snippet_generator = SnippetGenerator::create(&searcher, &*query, body_field)?;

        // Display results.
        let results: anyhow::Result<_> = top_docs
            .into_iter()
            .map(|(_score, doc_address)| {
                let retrieved_doc: TantivyDocument =
                    searcher.doc(doc_address).context("Could not get doc")?;

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
                let body_text = retrieved_doc
                    .get_first(body_field)
                    .unwrap()
                    .as_str()
                    .unwrap();
                let snippet = snippet_generator.snippet(body_text);

                // let explanation = query.explain(&searcher, doc_address)?;
                // println!("Explanation: {}", explanation.to_pretty_json());

                Ok(SearchResult {
                    title,
                    url,
                    snippet,
                })
            })
            .collect();

        Ok(results?)
    }
}

/// The result of a web search.
pub struct SearchResult {
    pub title: String,
    pub url: String,
    /// A relevant snippet from the page.
    pub snippet: Snippet,
}

pub async fn start() -> anyhow::Result<(Arc<Indexer>, mpsc::Sender<Page>)> {
    let indexer = Arc::new(Indexer::new(consts::SEARCH_INDEX_DIR).await?);
    let add_page_indexer = indexer.clone();
    let commit_indexer = indexer.clone();

    let (tx, mut rx) = mpsc::channel(1000);

    tokio::task::spawn(async move {
        while let Some(page) = rx.recv().await {
            if let Err(e) = add_page_indexer.add_page(&page) {
                let url = page.get_url();
                println!("ERROR: could not index page '{url}': {e}");
            }
        }
    });

    // Periodically commit.
    // NOTE: Committing can block, and is also non-async, so we use a dedicated thread.
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));

        let mut index_writer_wlock = commit_indexer.index_writer.write().unwrap();
        if let Err(e) = index_writer_wlock.commit() {
            println!("ERROR: could not commit index: {e}");
        }
    });

    Ok((indexer, tx))
}
