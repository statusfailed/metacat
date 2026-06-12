use std::collections::HashMap;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: RwLock<HashMap<Url, String>>,
}

impl DocumentStore {
    pub async fn set(&self, uri: Url, text: String) {
        self.documents.write().await.insert(uri, text);
    }

    pub async fn get(&self, uri: &Url) -> Option<String> {
        self.documents.read().await.get(uri).cloned()
    }
}
