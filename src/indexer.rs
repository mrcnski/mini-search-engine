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
    schema::{Schema, STORED, TEXT},
    Document, Index, IndexWriter, TantivyDocument,
};
use tokio::sync::mpsc;

use crate::consts;

pub struct Indexer {
    index: Index,
    index_writer: Arc<RwLock<IndexWriter>>,
    schema: Schema,
}

impl Indexer {
    pub async fn new(index_path: &str) -> anyhow::Result<Self> {
        let schema = Self::create_schema();
        let index = Self::create_index(schema.clone(), index_path).await?;

        let index_writer: Arc<RwLock<IndexWriter>> =
            Arc::new(RwLock::new(index.writer(50_000_000)?));

        Ok(Indexer {
            index,
            index_writer,
            schema,
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("description", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT | STORED);
        schema_builder.add_text_field("url", STORED);
        schema_builder.build()
    }

    async fn create_index(schema: Schema, index_path: &str) -> anyhow::Result<Index> {
        // Delete any existing index.
        // TODO: Remove this once we finalize the schema.
        let _ = tokio::fs::remove_dir_all(index_path).await?;

        // Create index directory if it doesn't exist
        tokio::fs::create_dir_all(index_path).await?;

        Ok(Index::create_in_dir(index_path, schema.clone())?)
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
        // Open a searcher.
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        // Get fields.
        let schema = &self.schema;
        let title_field = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let description_field = schema.get_field("description").unwrap();

        // Create a query parser. Weight some fields for hopefully more relevant results.
        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![title_field, body_field, description_field],
        );
        query_parser.set_field_boost(title_field, 2.0);
        query_parser.set_field_boost(body_field, 1.0);
        query_parser.set_field_boost(description_field, 0.5);

        // Parse the query.
        let query = query_parser.parse_query(query_str)?;

        // Collect top results.
        let top_docs = searcher.search(&query, &TopDocs::with_limit(num_docs))?;

        // Display results.
        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
            println!("Score: {}", score);
            println!("Document: {:?}", retrieved_doc.to_json(&schema));

            let explanation = query.explain(&searcher, doc_address)?;
            println!("Explanation: {}", explanation.to_pretty_json());
        }

        // TODO
        Ok(vec![])
    }
}

/// The result of a web search.
pub struct SearchResult {
    pub title: String,
    pub url: String,
    /// A relevant snippet from the page.
    pub snippet: String,
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
