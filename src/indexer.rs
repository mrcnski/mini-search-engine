use spider::page::Page;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tantivy::{
    doc,
    schema::{Schema, STORED, TEXT},
    Index, IndexWriter,
};
use tokio::{sync::mpsc, task::JoinSet};

use crate::{consts, utils::log};

struct Indexer {
    index: Index,
    index_writer: Arc<RwLock<IndexWriter>>,
    schema: Schema,
}

impl Indexer {
    pub async fn new(index_path: &str) -> anyhow::Result<Self> {
        // Delete any existing index.
        // TODO: Remove this once we finalize the schema.
        let _ = tokio::fs::remove_dir_all(index_path).await?;

        // Create index directory if it doesn't exist
        tokio::fs::create_dir_all(index_path).await?;

        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("url", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(index_path, schema.clone())?;
        let index_writer: Arc<RwLock<IndexWriter>> =
            Arc::new(RwLock::new(index.writer(50_000_000)?));

        Ok(Indexer {
            index,
            index_writer,
            schema,
        })
    }

    pub fn add_page(&self, url: &str, content: &str) -> anyhow::Result<()> {
        let url_field = self.schema.get_field("url").unwrap();
        let content_field = self.schema.get_field("content").unwrap();

        let index_writer_wlock = self.index_writer.write().unwrap();
        index_writer_wlock.add_document(doc!(
            url_field => url,
            content_field => content,
        ))?;

        Ok(())
    }
}

pub async fn start() -> anyhow::Result<mpsc::Sender<Page>> {
    let indexer = Indexer::new(consts::SEARCH_INDEX_DIR).await?;
    let index_writer = indexer.index_writer.clone();

    let (tx, mut rx) = mpsc::channel(1000);

    tokio::task::spawn(async move {
        let mut indexer_tasks: JoinSet<()> = JoinSet::new();

        while let Some(_page) = rx.recv().await {
            indexer_tasks.spawn(async move {});

            // Limit the number of tasks.
            while indexer_tasks.len() > 100 {
                if let Some(Err(e)) = indexer_tasks.join_next().await {
                    log(&format!("WARNING: could not index: {e}")).await;
                }
            }
        }
    });

    // Periodically commit.
    std::thread::spawn(move || {
        let index_writer = indexer.index_writer.clone();

        loop {
            std::thread::sleep(Duration::from_millis(500));

            let mut index_writer_wlock = index_writer.write().unwrap();
            // NOTE: This can block.
            if let Err(e) = index_writer_wlock.commit() {
                println!("ERROR: could not commit index: {e}");
            }
        }
    });

    Ok(tx)
}
