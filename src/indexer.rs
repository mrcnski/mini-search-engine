use tantivy::{
    doc,
    schema::{Schema, STORED, TEXT},
    Index,
};

use crate::consts;

struct Indexer {
    index: Index,
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

        Ok(Indexer { index, schema })
    }

    pub fn add_page(&self, url: &str, content: &str) -> anyhow::Result<()> {
        let mut index_writer = self.index.writer(50_000_000)?;
        let url_field = self.schema.get_field("url").unwrap();
        let content_field = self.schema.get_field("content").unwrap();

        index_writer.add_document(doc!(
            url_field => url,
            content_field => content,
        ))?;

        index_writer.commit()?;
        Ok(())
    }
}

// TODO: Log any errors in indexing, try to restart indexer.
pub async fn start() -> anyhow::Result<()> {
    let _indexer = Indexer::new(consts::SEARCH_INDEX_DIR).await?;

    Ok(())
}
