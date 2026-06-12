use tower_lsp::lsp_types::{
    HoverProviderCapability, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

/// Capabilities are the LSP feature list this server advertises.
///
/// Add new protocol features here first, then implement the matching
/// `LanguageServer` method in `server.rs`.
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    }
}
