mod backend;
mod code_actions;
mod completion;
mod definition;
mod diagnostics;
mod documents;
mod hover;
mod project;
mod ranking;
mod references;
mod rename;
mod syntax;
mod symbols;
mod workspace;
mod workspace_symbols;

use tower_lsp::{LspService, Server};

use crate::backend::Backend;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);

    Server::new(stdin, stdout, socket).serve(service).await;
}
