use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionOrCommand, CodeActionParams,
    CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse,
    DocumentSymbolParams, DocumentSymbolResponse,
    RenameParams, WorkspaceEdit,
    ReferenceParams,
    Hover, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, MessageType,
    ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
    SymbolInformation, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};

use crate::diagnostics::publish_diagnostics;
use crate::code_actions::code_actions_for;
use crate::documents::DocumentStore;
use crate::completion::completions_for;
use crate::definition::definition_for;
use crate::hover::hover_for;
use crate::references::references_for;
use crate::rename::rename_for;
use crate::symbols::document_symbols_for;
use crate::workspace::WorkspaceState;
use crate::workspace_symbols::workspace_symbols_for;

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<DocumentStore>>,
    workspace: Arc<RwLock<WorkspaceState>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(DocumentStore::default())),
            workspace: Arc::new(RwLock::new(WorkspaceState::default())),
        }
    }

    async fn refresh_diagnostics(&self, uri: tower_lsp::lsp_types::Url) {
        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };

        let Some(source) = source else {
            self.client.publish_diagnostics(uri, Vec::new(), None).await;
            return;
        };

        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        publish_diagnostics(&self.client, uri, &source, &project_index).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions::default()),
                code_action_provider: Some(tower_lsp::lsp_types::CodeActionProviderCapability::Simple(true)),
                document_symbol_provider: Some(tower_lsp::lsp_types::OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                rename_provider: Some(tower_lsp::lsp_types::OneOf::Left(true)),
                workspace_symbol_provider: Some(tower_lsp::lsp_types::OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: tower_lsp::lsp_types::InitializedParams) {
        let _ = self
            .client
            .log_message(MessageType::INFO, "spiders-css-lsp initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        let uri = document.uri;

        {
            let mut documents = self.documents.write().await;
            documents.open(uri.clone(), document.text);
        }

        {
            let source = {
                let documents = self.documents.read().await;
                documents.get(&uri).map(str::to_owned)
            };
            if let Some(source) = source {
                let mut workspace = self.workspace.write().await;
                workspace.upsert_document(&uri, &source);
            }
        }

        self.refresh_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };

        {
            let mut documents = self.documents.write().await;
            documents.update(uri.clone(), change.text);
        }

        {
            let source = {
                let documents = self.documents.read().await;
                documents.get(&uri).map(str::to_owned)
            };
            if let Some(source) = source {
                let mut workspace = self.workspace.write().await;
                workspace.upsert_document(&uri, &source);
            }
        }

        self.refresh_diagnostics(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        {
            let mut documents = self.documents.write().await;
            documents.close(&uri);
        }

        {
            let mut workspace = self.workspace.write().await;
            workspace.remove_document(&uri);
        }

        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let text_document_position = params.text_document_position_params;
        let uri = text_document_position.text_document.uri;
        let position = text_document_position.position;

        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };

        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(source.and_then(|source| hover_for(&uri, &source, position, &project_index)))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let text_document_position = params.text_document_position;
        let uri = text_document_position.text_document.uri;
        let position = text_document_position.position;

        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };
        let workspace = self.workspace.read().await;

        Ok(source.and_then(|source| completions_for(&uri, &source, position, workspace.project_index())))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };

        Ok(source.map(|source| DocumentSymbolResponse::Nested(document_symbols_for(&source))))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let text_document_position = params.text_document_position_params;
        let uri = text_document_position.text_document.uri;
        let position = text_document_position.position;
        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };
        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(source.and_then(|source| definition_for(&uri, &source, position, &project_index)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<tower_lsp::lsp_types::Location>>> {
        let text_document_position = params.text_document_position;
        let uri = text_document_position.text_document.uri;
        let position = text_document_position.position;
        let include_declaration = params.context.include_declaration;
        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };
        let documents = {
            let documents = self.documents.read().await;
            documents.snapshot()
        };
        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(source.map(|source| {
            references_for(
                &uri,
                &source,
                position,
                include_declaration,
                &project_index,
                &documents,
            )
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let text_document_position = params.text_document_position;
        let uri = text_document_position.text_document.uri;
        let position = text_document_position.position;
        let new_name = params.new_name;
        let source = {
            let documents = self.documents.read().await;
            documents.get(&uri).map(str::to_owned)
        };

        let documents = {
            let documents = self.documents.read().await;
            documents.snapshot()
        };
        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(source.and_then(|source| {
            rename_for(&uri, &source, position, &new_name, &project_index, &documents)
        }))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        let uri = params.text_document.uri;
        let diagnostics = params.context.diagnostics;
        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(Some(code_actions_for(&uri, &project_index, &diagnostics)))
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        let project_index = {
            let workspace = self.workspace.read().await;
            workspace.project_index().clone()
        };

        Ok(Some(workspace_symbols_for(&params.query, &project_index)))
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
