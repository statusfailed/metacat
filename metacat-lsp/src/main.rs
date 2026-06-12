use metacat::check::check;
use metacat::theory::{Theory, TheorySet};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Diagnostic, DiagnosticSeverity,
    InitializeParams, InitializeResult, MessageType, Position, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..ServerCapabilities::default()
            },
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
        self.documents.write().await.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        self.documents
            .write()
            .await
            .insert(uri.clone(), change.text.clone());
        self.publish_diagnostics(uri, &change.text).await;
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let diagnostics = validate_document(text)
            .err()
            .map(|message| vec![document_diagnostic(message)])
            .unwrap_or_default();

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

fn validate_document(text: &str) -> std::result::Result<(), String> {
    let theories = TheorySet::from_text(text).map_err(|error| error.to_string())?;

    for (theory_id, theory) in &theories.theories {
        let Theory::Theory { arrows, .. } = theory else {
            continue;
        };

        for declaration in arrows.values().filter(|arrow| arrow.definition.is_some()) {
            let mut term = declaration
                .definition
                .clone()
                .expect("filtered to definitional arrows");
            let (source, target) = declaration.type_maps.clone();
            check(theory, source, target, &mut term).map_err(|error| {
                format!(
                    "Checking '{}.{}' failed: {}",
                    theory_id, declaration.name, error
                )
            })?;
        }
    }

    Ok(())
}

fn document_diagnostic(message: String) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("metacat".to_string()),
        message,
        ..Diagnostic::default()
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
