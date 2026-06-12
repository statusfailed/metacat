use crate::capabilities::server_capabilities;
use crate::diagnostics::diagnostics_for_document;
use crate::documents::DocumentStore;
use crate::hover::hover_at_position;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Hover, HoverParams, InitializeParams,
    InitializeResult, MessageType,
};
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct Backend {
    client: Client,
    documents: DocumentStore,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DocumentStore::default(),
        }
    }

    async fn publish_diagnostics(&self, uri: tower_lsp::lsp_types::Url, text: &str) {
        self.client
            .publish_diagnostics(uri, diagnostics_for_document(text), None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: None,
        })
    }

    async fn initialized(&self, _: tower_lsp::lsp_types::InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "metacat-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.set(uri.clone(), text.clone()).await;
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.documents.set(uri.clone(), change.text.clone()).await;
        self.publish_diagnostics(uri, &change.text).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(text) = self.documents.get(&uri).await else {
            return Ok(None);
        };

        Ok(hover_at_position(&text, position))
    }
}
