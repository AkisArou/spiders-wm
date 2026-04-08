use crate::{IpcHandler, IpcResponse, IpcServerHandleResult, IpcServerState, UnknownClientError};

pub fn resolve_ipc_request<H>(
    server: &mut IpcServerState,
    client_id: crate::IpcClientId,
    request: crate::IpcRequest,
    handler: &mut H,
) -> Result<IpcResponse, ResolveIpcRequestError<H::Error>>
where
    H: IpcHandler,
{
    match server.handle_request(client_id, request)? {
        IpcServerHandleResult::Query { client_id, request_id, query } => {
            let response = handler.handle_query(query).map_err(ResolveIpcRequestError::Handler)?;
            Ok(server.query_response(client_id, request_id, response)?)
        }
        IpcServerHandleResult::Command { client_id, request_id, command } => {
            handler.handle_command(command).map_err(ResolveIpcRequestError::Handler)?;
            Ok(server.command_accepted(client_id, request_id)?)
        }
        IpcServerHandleResult::Debug { client_id, request_id, request } => {
            let response =
                handler.handle_debug(request).map_err(ResolveIpcRequestError::Handler)?;
            Ok(server.debug_response(client_id, request_id, response)?)
        }
        IpcServerHandleResult::Response { response, .. } => Ok(response),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveIpcRequestError<E> {
    #[error(transparent)]
    UnknownClient(#[from] UnknownClientError),
    #[error(transparent)]
    Handler(E),
}

#[cfg(test)]
mod tests {
    use spiders_core::command::WmCommand;
    use spiders_core::query::{QueryRequest, QueryResponse};

    use super::*;
    use crate::{
        DebugRequest, DebugResponse, IpcClientMessage, IpcEnvelope, IpcServerMessage,
        IpcSubscriptionTopic,
    };

    struct TestHandler {
        commands: Vec<WmCommand>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self { commands: Vec::new() }
        }
    }

    impl IpcHandler for TestHandler {
        type Error = std::convert::Infallible;

        fn handle_query(&mut self, _query: QueryRequest) -> Result<QueryResponse, Self::Error> {
            Ok(QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]))
        }

        fn handle_command(&mut self, command: WmCommand) -> Result<(), Self::Error> {
            self.commands.push(command);
            Ok(())
        }

        fn handle_debug(&mut self, _request: DebugRequest) -> Result<DebugResponse, Self::Error> {
            Ok(DebugResponse::DumpWritten { kind: crate::DebugDumpKind::WmState, path: None })
        }
    }

    #[test]
    fn resolve_routes_query_command_and_subscription_responses() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();
        let mut handler = TestHandler::new();

        let query_response = resolve_ipc_request(
            &mut server,
            client_id,
            IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::WorkspaceNames))
                .with_request_id("req-query"),
            &mut handler,
        )
        .unwrap();

        assert_eq!(
            query_response,
            IpcEnvelope {
                request_id: Some("req-query".into()),
                message: IpcServerMessage::Query(QueryResponse::WorkspaceNames(vec![
                    "1".into(),
                    "2".into(),
                ])),
            }
        );

        let command_response = resolve_ipc_request(
            &mut server,
            client_id,
            IpcEnvelope::new(IpcClientMessage::Command(WmCommand::ReloadConfig))
                .with_request_id("req-command"),
            &mut handler,
        )
        .unwrap();

        assert_eq!(handler.commands, vec![WmCommand::ReloadConfig]);
        assert_eq!(
            command_response,
            IpcEnvelope {
                request_id: Some("req-command".into()),
                message: IpcServerMessage::CommandAccepted,
            }
        );

        let subscribe_response = resolve_ipc_request(
            &mut server,
            client_id,
            IpcEnvelope::new(IpcClientMessage::subscribe([
                IpcSubscriptionTopic::Focus,
                IpcSubscriptionTopic::Focus,
            ]))
            .with_request_id("req-subscribe"),
            &mut handler,
        )
        .unwrap();

        assert_eq!(
            subscribe_response,
            IpcEnvelope {
                request_id: Some("req-subscribe".into()),
                message: IpcServerMessage::Subscribed { topics: vec![IpcSubscriptionTopic::Focus] },
            }
        );
    }
}
