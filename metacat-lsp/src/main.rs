mod analysis;
mod arrow_details;
mod capabilities;
mod diagnostics;
mod documents;
mod hover;
mod project;
mod server;
mod syntax;

use server::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(Backend::new)
        .custom_method("metacat/arrowDetails", Backend::arrow_details)
        .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
