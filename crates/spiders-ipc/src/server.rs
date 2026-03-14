use std::collections::BTreeMap;
use std::os::unix::net::UnixStream;

use spiders_shared::api::{CompositorEvent, QueryRequest, QueryResponse, WmAction};

use crate::{
    recv_request, send_response, IpcRequest, IpcResponse, IpcSession, IpcSessionHandleResult,
    IpcTransportError,
};

pub type IpcClientId = u64;

#[derive(Debug, Clone, Default)]
pub struct IpcServerState {
    sessions: BTreeMap<IpcClientId, IpcSession>,
    next_client_id: IpcClientId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcServerHandleResult {
    Query {
        client_id: IpcClientId,
        request_id: Option<String>,
        query: QueryRequest,
    },
    Action {
        client_id: IpcClientId,
        request_id: Option<String>,
        action: WmAction,
    },
    Response {
        client_id: IpcClientId,
        response: IpcResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownClientError {
    pub client_id: IpcClientId,
}

#[derive(Debug, thiserror::Error)]
pub enum IpcServeError {
    #[error(transparent)]
    UnknownClient(#[from] UnknownClientError),
    #[error(transparent)]
    Transport(#[from] IpcTransportError),
}

impl std::fmt::Display for UnknownClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown IPC client {}", self.client_id)
    }
}

impl std::error::Error for UnknownClientError {}

impl IpcServerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_client(&mut self) -> IpcClientId {
        let client_id = self.next_client_id;
        self.next_client_id += 1;
        self.sessions.insert(client_id, IpcSession::new());
        client_id
    }

    pub fn remove_client(&mut self, client_id: IpcClientId) -> Option<IpcSession> {
        self.sessions.remove(&client_id)
    }

    pub fn client_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn handle_request(
        &mut self,
        client_id: IpcClientId,
        request: IpcRequest,
    ) -> Result<IpcServerHandleResult, UnknownClientError> {
        let session = self
            .sessions
            .get_mut(&client_id)
            .ok_or(UnknownClientError { client_id })?;

        Ok(match session.handle_request(request) {
            IpcSessionHandleResult::Query { request_id, query } => IpcServerHandleResult::Query {
                client_id,
                request_id,
                query,
            },
            IpcSessionHandleResult::Action { request_id, action } => {
                IpcServerHandleResult::Action {
                    client_id,
                    request_id,
                    action,
                }
            }
            IpcSessionHandleResult::Response(response) => IpcServerHandleResult::Response {
                client_id,
                response,
            },
        })
    }

    pub fn query_response(
        &self,
        client_id: IpcClientId,
        request_id: Option<String>,
        response: QueryResponse,
    ) -> Result<IpcResponse, UnknownClientError> {
        self.sessions
            .get(&client_id)
            .map(|session| session.query_response(request_id, response))
            .ok_or(UnknownClientError { client_id })
    }

    pub fn action_accepted(
        &self,
        client_id: IpcClientId,
        request_id: Option<String>,
    ) -> Result<IpcResponse, UnknownClientError> {
        self.sessions
            .get(&client_id)
            .map(|session| session.action_accepted(request_id))
            .ok_or(UnknownClientError { client_id })
    }

    pub fn error_response(
        &self,
        client_id: IpcClientId,
        request_id: Option<String>,
        message: impl Into<String>,
    ) -> Result<IpcResponse, UnknownClientError> {
        self.sessions
            .get(&client_id)
            .map(|session| session.error_response(request_id, message.into()))
            .ok_or(UnknownClientError { client_id })
    }

    pub fn broadcast_event(&self, event: CompositorEvent) -> Vec<(IpcClientId, IpcResponse)> {
        self.sessions
            .iter()
            .filter_map(|(client_id, session)| {
                session
                    .event_response(event.clone())
                    .map(|response| (*client_id, response))
            })
            .collect()
    }

    pub fn serve_stream<F, E>(
        &mut self,
        client_id: IpcClientId,
        stream: &mut UnixStream,
        mut responder: F,
    ) -> Result<IpcResponse, E>
    where
        F: FnMut(IpcServerHandleResult) -> Result<IpcResponse, E>,
        E: From<IpcServeError>,
    {
        let request = recv_request(stream).map_err(IpcServeError::from)?;
        let result = self
            .handle_request(client_id, request)
            .map_err(IpcServeError::from)?;
        let response = match result {
            IpcServerHandleResult::Response { response, .. } => response,
            other => responder(other)?,
        };

        send_response(stream, &response).map_err(IpcServeError::from)?;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{IpcClientMessage, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic};

    use super::*;

    #[test]
    fn server_allocates_and_removes_clients() {
        let mut server = IpcServerState::new();

        let first = server.add_client();
        let second = server.add_client();

        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(server.client_count(), 2);
        assert!(server.remove_client(first).is_some());
        assert_eq!(server.client_count(), 1);
    }

    #[test]
    fn server_routes_query_requests_to_compositor_layer() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        let result = server.handle_request(
            client_id,
            IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::State)).with_request_id("req-1"),
        );

        assert_eq!(
            result,
            Ok(IpcServerHandleResult::Query {
                client_id,
                request_id: Some("req-1".into()),
                query: QueryRequest::State,
            })
        );
    }

    #[test]
    fn server_routes_action_requests_to_compositor_layer() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        let result = server.handle_request(
            client_id,
            IpcEnvelope::new(IpcClientMessage::Action(WmAction::ReloadConfig))
                .with_request_id("req-2"),
        );

        assert_eq!(
            result,
            Ok(IpcServerHandleResult::Action {
                client_id,
                request_id: Some("req-2".into()),
                action: WmAction::ReloadConfig,
            })
        );
    }

    #[test]
    fn server_returns_immediate_subscription_responses() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        let result = server.handle_request(
            client_id,
            IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Layout])),
        );

        assert_eq!(
            result,
            Ok(IpcServerHandleResult::Response {
                client_id,
                response: IpcEnvelope::new(IpcServerMessage::Subscribed {
                    topics: vec![IpcSubscriptionTopic::Layout],
                }),
            })
        );
    }

    #[test]
    fn server_builds_query_and_action_responses_for_known_clients() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        assert_eq!(
            server.query_response(
                client_id,
                Some("req-3".into()),
                QueryResponse::TagNames(vec!["1".into()]),
            ),
            Ok(
                IpcEnvelope::new(IpcServerMessage::Query(QueryResponse::TagNames(vec![
                    "1".into()
                ],)))
                .with_request_id("req-3")
            )
        );
        assert_eq!(
            server.action_accepted(client_id, Some("req-4".into())),
            Ok(IpcEnvelope::new(IpcServerMessage::ActionAccepted).with_request_id("req-4"))
        );
    }

    #[test]
    fn server_broadcasts_events_only_to_matching_clients() {
        let mut server = IpcServerState::new();
        let layout_client = server.add_client();
        let all_client = server.add_client();
        let focus_client = server.add_client();

        server
            .handle_request(
                layout_client,
                IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Layout])),
            )
            .unwrap();
        server
            .handle_request(
                all_client,
                IpcEnvelope::new(IpcClientMessage::subscribe_all()),
            )
            .unwrap();
        server
            .handle_request(
                focus_client,
                IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Focus])),
            )
            .unwrap();

        let responses = server.broadcast_event(CompositorEvent::LayoutChange {
            workspace_id: None,
            layout: None,
        });

        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].0, layout_client);
        assert_eq!(responses[1].0, all_client);
        assert!(matches!(
            &responses[0].1,
            IpcEnvelope {
                message: IpcServerMessage::Event { .. },
                ..
            }
        ));
    }

    #[test]
    fn server_returns_unknown_client_errors() {
        let mut server = IpcServerState::new();

        let error = server
            .handle_request(7, IpcEnvelope::new(IpcClientMessage::subscribe_all()))
            .unwrap_err();

        assert_eq!(error, UnknownClientError { client_id: 7 });
        assert_eq!(error.to_string(), "unknown IPC client 7");
    }

    #[test]
    fn server_serve_stream_replies_with_subscription_ack() {
        let path = unique_socket_path("serve-subscribe");
        let listener = UnixListener::bind(&path).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();

        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        crate::send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Layout])),
        )
        .unwrap();

        let response = server
            .serve_stream::<_, IpcServeError>(client_id, &mut server_stream, |_| unreachable!())
            .unwrap();

        assert_eq!(
            response,
            IpcEnvelope::new(IpcServerMessage::Subscribed {
                topics: vec![IpcSubscriptionTopic::Layout],
            })
        );

        let decoded = crate::recv_response(&client).unwrap();
        assert_eq!(decoded, response);

        drop(server_stream);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn server_serve_stream_uses_responder_for_query_requests() {
        let path = unique_socket_path("serve-query");
        let listener = UnixListener::bind(&path).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();

        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        crate::send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::TagNames))
                .with_request_id("req-10"),
        )
        .unwrap();

        let response =
            server
                .serve_stream::<_, IpcServeError>(client_id, &mut server_stream, |result| {
                    match result {
                        IpcServerHandleResult::Query {
                            request_id,
                            query: QueryRequest::TagNames,
                            ..
                        } => Ok::<IpcResponse, IpcServeError>(
                            IpcEnvelope::new(IpcServerMessage::Query(QueryResponse::TagNames(
                                vec!["1".into(), "2".into()],
                            )))
                            .with_request_id(request_id.unwrap_or_default()),
                        ),
                        other => panic!("unexpected serve result: {other:?}"),
                    }
                })
                .unwrap();

        assert_eq!(
            response,
            IpcEnvelope::new(IpcServerMessage::Query(QueryResponse::TagNames(vec![
                "1".into(),
                "2".into(),
            ])))
            .with_request_id("req-10")
        );

        let decoded = crate::recv_response(&client).unwrap();
        assert_eq!(decoded, response);

        drop(server_stream);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn server_serve_stream_uses_responder_for_action_requests() {
        let path = unique_socket_path("serve-action");
        let listener = UnixListener::bind(&path).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();

        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        crate::send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::ReloadConfig))
                .with_request_id("req-11"),
        )
        .unwrap();

        let response =
            server
                .serve_stream::<_, IpcServeError>(client_id, &mut server_stream, |result| {
                    match result {
                        IpcServerHandleResult::Action {
                            request_id,
                            action: WmAction::ReloadConfig,
                            ..
                        } => Ok::<IpcResponse, IpcServeError>(
                            IpcEnvelope::new(IpcServerMessage::ActionAccepted)
                                .with_request_id(request_id.unwrap_or_default()),
                        ),
                        other => panic!("unexpected serve result: {other:?}"),
                    }
                })
                .unwrap();

        assert_eq!(
            response,
            IpcEnvelope::new(IpcServerMessage::ActionAccepted).with_request_id("req-11")
        );

        let decoded = crate::recv_response(&client).unwrap();
        assert_eq!(decoded, response);

        drop(server_stream);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    fn unique_socket_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-ipc-server-{label}-{nanos}.sock"))
    }
}
