#![allow(dead_code)]

use std::collections::BTreeMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use spiders_core::command::WmCommand;
use spiders_core::event::WmEvent;
use spiders_core::query::{QueryRequest, QueryResponse, query_response_for_model};
use spiders_ipc::{
    IpcClientId, IpcCodecError, IpcRequest, IpcResponse, IpcServerHandleResult, IpcServerState,
    IpcTransportError, UnknownClientError, bind_listener, recv_request, send_response,
};
use tracing::{debug, warn};

use crate::state::SpidersWm;

impl SpidersWm {
    pub fn broadcast_runtime_events(&mut self, events: impl IntoIterator<Item = WmEvent>) {
        for event in events {
            self.broadcast_ipc_event(event);
        }
    }

    pub fn ipc_add_client(&mut self) -> IpcClientId {
        self.ipc_server.add_client()
    }

    pub fn ipc_remove_client(&mut self, client_id: IpcClientId) {
        self.ipc_clients.remove(&client_id);
        self.ipc_server.remove_client(client_id);
    }

    pub fn handle_ipc_request(
        &mut self,
        client_id: IpcClientId,
        request: IpcRequest,
    ) -> Result<IpcResponse, UnknownClientError> {
        match self.ipc_server.handle_request(client_id, request)? {
            IpcServerHandleResult::Query { client_id, request_id, query } => {
                debug!(client_id, request_id = ?request_id, query = ?query, "wm handling IPC query");
                let response = self.query_ipc(query);
                self.ipc_server.query_response(client_id, request_id, response)
            }
            IpcServerHandleResult::Command { client_id, request_id, command } => {
                debug!(client_id, request_id = ?request_id, command = ?command, "wm handling IPC command");
                self.execute_wm_command(command);
                self.ipc_server.command_accepted(client_id, request_id)
            }
            IpcServerHandleResult::Response { response, .. } => Ok(response),
        }
    }

    pub fn query_ipc(&self, query: QueryRequest) -> QueryResponse {
        query_response_for_model(&self.model, query)
    }

    pub fn register_ipc_client_stream(&mut self, stream: UnixStream) -> std::io::Result<()> {
        let client_id = self.ipc_add_client();
        let writer = stream.try_clone()?;
        self.ipc_clients.insert(client_id, writer);

        self.event_loop
            .insert_source(Generic::new(stream, Interest::READ, Mode::Level), move |_, _, state| {
                state.handle_ipc_client_io(client_id)
            })
            .expect("failed to register IPC client stream");

        Ok(())
    }

    pub fn handle_ipc_client_io(
        &mut self,
        client_id: IpcClientId,
    ) -> Result<PostAction, std::io::Error> {
        match serve_ipc_client_stream(self, client_id) {
            Ok(_) => Ok(PostAction::Continue),
            Err(WmIpcStreamError::Transport(IpcTransportError::Io(error)))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                self.ipc_remove_client(client_id);
                Ok(PostAction::Remove)
            }
            Err(WmIpcStreamError::Transport(IpcTransportError::Codec(
                IpcCodecError::EmptyFrame,
            ))) => {
                self.ipc_remove_client(client_id);
                Ok(PostAction::Remove)
            }
            Err(WmIpcStreamError::UnknownClient(_)) => Ok(PostAction::Remove),
            Err(WmIpcStreamError::Transport(IpcTransportError::Codec(error))) => {
                warn!(client_id, %error, "discarding malformed IPC request");
                let response = self
                    .ipc_server
                    .error_response(client_id, None, error.to_string())
                    .map_err(std::io::Error::other)?;
                let Some(stream) = self.ipc_clients.get_mut(&client_id) else {
                    self.ipc_remove_client(client_id);
                    return Ok(PostAction::Remove);
                };
                match send_response(stream, &response).map_err(stream_io_error) {
                    Ok(()) => Ok(PostAction::Continue),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::UnexpectedEof
                        ) =>
                    {
                        self.ipc_remove_client(client_id);
                        Ok(PostAction::Remove)
                    }
                    Err(error) => Err(error),
                }
            }
            Err(WmIpcStreamError::Transport(IpcTransportError::Io(error))) => Err(error),
        }
    }

    pub fn broadcast_ipc_event(&mut self, event: WmEvent) {
        let stale_clients =
            broadcast_ipc_event_to_clients(&self.ipc_server, &mut self.ipc_clients, event);

        for client_id in stale_clients {
            self.ipc_remove_client(client_id);
        }
    }

    pub fn emit_config_reloaded(&mut self) {
        self.broadcast_ipc_event(WmEvent::ConfigReloaded);
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WmIpcStreamError {
    #[error(transparent)]
    Transport(#[from] IpcTransportError),
    #[error(transparent)]
    UnknownClient(#[from] UnknownClientError),
}

pub(crate) fn init_ipc_listener(event_loop: &mut EventLoop<'static, SpidersWm>) -> Option<PathBuf> {
    if std::env::var_os("SPIDERS_WM_DISABLE_IPC").is_some() {
        return None;
    }

    let socket_path = configured_ipc_socket_path();
    let listener = match bind_wm_ipc_listener(&socket_path) {
        Ok(listener) => listener,
        Err(error) => {
            warn!(path = %socket_path.display(), %error, "failed to create wm IPC socket");
            return None;
        }
    };

    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            move |_, listener, state| {
                accept_pending_ipc_clients(state, listener);
                Ok(PostAction::Continue)
            },
        )
        .expect("failed to register IPC listener socket");

    Some(socket_path)
}

pub(crate) fn serve_ipc_client_stream(
    state: &mut SpidersWm,
    client_id: IpcClientId,
) -> Result<IpcResponse, WmIpcStreamError> {
    let request = {
        let stream =
            state.ipc_clients.get_mut(&client_id).ok_or(UnknownClientError { client_id })?;
        recv_request(stream)?
    };
    let response = state.handle_ipc_request(client_id, request)?;
    let stream = state.ipc_clients.get_mut(&client_id).ok_or(UnknownClientError { client_id })?;
    send_response(stream, &response)?;
    Ok(response)
}

pub(crate) fn broadcast_ipc_event_to_clients(
    server: &IpcServerState,
    clients: &mut BTreeMap<IpcClientId, UnixStream>,
    event: WmEvent,
) -> Vec<IpcClientId> {
    let mut stale_clients = Vec::new();

    for (client_id, response) in server.broadcast_event(event) {
        let Some(stream) = clients.get_mut(&client_id) else {
            stale_clients.push(client_id);
            continue;
        };

        if let Err(error) = send_response(stream, &response) {
            warn!(client_id, %error, "failed to send IPC event response");
            stale_clients.push(client_id);
        }
    }

    stale_clients
}

fn configured_ipc_socket_path() -> PathBuf {
    std::env::var_os("SPIDERS_WM_IPC_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(default_ipc_socket_path)
}

fn default_ipc_socket_path() -> PathBuf {
    let base =
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(format!("spiders-wm-{}.sock", std::process::id()))
}

fn bind_wm_ipc_listener(socket_path: &PathBuf) -> Result<UnixListener, IpcTransportError> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = bind_listener(socket_path)?;
    listener.set_nonblocking(true)?;
    debug!(path = %socket_path.display(), "bound wm IPC socket");
    Ok(listener)
}

fn accept_pending_ipc_clients(state: &mut SpidersWm, listener: &UnixListener) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    warn!(%error, "failed to set IPC client stream nonblocking");
                    continue;
                }

                if let Err(error) = state.register_ipc_client_stream(stream) {
                    warn!(%error, "failed to register IPC client stream");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => {
                warn!(%error, "failed to accept IPC client stream");
                break;
            }
        }
    }
}

fn stream_io_error(error: IpcTransportError) -> std::io::Error {
    match error {
        IpcTransportError::Io(error) => error,
        IpcTransportError::Codec(error) => {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error)
        }
    }
}

pub(crate) fn resolve_ipc_request<QueryHandler, CommandHandler>(
    server: &mut IpcServerState,
    client_id: IpcClientId,
    request: IpcRequest,
    mut query_handler: QueryHandler,
    mut command_handler: CommandHandler,
) -> Result<IpcResponse, UnknownClientError>
where
    QueryHandler: FnMut(QueryRequest) -> QueryResponse,
    CommandHandler: FnMut(WmCommand),
{
    match server.handle_request(client_id, request)? {
        IpcServerHandleResult::Query { client_id, request_id, query } => {
            server.query_response(client_id, request_id, query_handler(query))
        }
        IpcServerHandleResult::Command { client_id, request_id, command } => {
            command_handler(command);
            server.command_accepted(client_id, request_id)
        }
        IpcServerHandleResult::Response { response, .. } => Ok(response),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_core::command::WmCommand;
    use spiders_core::query::{
        QueryRequest, QueryResponse, query_response_for_model, state_snapshot_for_model,
    };
    use spiders_core::types::WindowMode;
    use spiders_core::wm::WmModel;
    use spiders_core::{OutputId as SharedOutputId, WindowId, WorkspaceId as SharedWorkspaceId};
    use spiders_core::{OutputId, WorkspaceId};
    use spiders_ipc::{IpcClientMessage, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic};
    use spiders_ipc::{bind_listener, connect, recv_response, send_request};

    use super::*;

    fn sample_model() -> WmModel {
        let mut model = WmModel::default();

        model.upsert_workspace(WorkspaceId::from("1"), "1".into());
        model.upsert_workspace(WorkspaceId::from("2"), "2".into());
        model.set_current_workspace(WorkspaceId::from("1"));
        model.upsert_output(
            OutputId::from("output-1"),
            "HDMI-A-1",
            1920,
            1080,
            Some(WorkspaceId::from("1")),
        );
        model.attach_workspace_to_output(WorkspaceId::from("1"), OutputId::from("output-1"));
        model.set_current_output(OutputId::from("output-1"));
        model.insert_window(
            WindowId::from("win-1"),
            Some(WorkspaceId::from("1")),
            Some(OutputId::from("output-1")),
        );
        let window = model.windows.get_mut(&WindowId::from("win-1")).unwrap();
        window.app_id = Some("foot".into());
        window.title = Some("terminal".into());
        window.mapped = true;
        window.focused = true;
        model.focused_window_id = Some(WindowId::from("win-1"));

        model
    }

    #[test]
    fn state_snapshot_tracks_shared_query_state() {
        let snapshot = state_snapshot_for_model(&sample_model());

        assert_eq!(snapshot.current_workspace_id, Some(SharedWorkspaceId::from("1")));
        assert_eq!(snapshot.current_output_id, Some(SharedOutputId::from("output-1")));
        assert_eq!(snapshot.focused_window_id, Some(WindowId::from("win-1")));
        assert_eq!(snapshot.workspace_names, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(snapshot.visible_window_ids, vec![WindowId::from("win-1")]);
        assert_eq!(snapshot.outputs.len(), 1);
        assert_eq!(snapshot.workspaces.len(), 2);
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].mode, WindowMode::Tiled);
    }

    #[test]
    fn query_response_for_model_returns_expected_variants() {
        let model = sample_model();

        assert!(matches!(
            query_response_for_model(&model, QueryRequest::State),
            QueryResponse::State(_)
        ));
        assert!(matches!(
            query_response_for_model(&model, QueryRequest::FocusedWindow),
            QueryResponse::FocusedWindow(Some(window)) if window.id == WindowId::from("win-1")
        ));
        assert!(matches!(
            query_response_for_model(&model, QueryRequest::CurrentWorkspace),
            QueryResponse::CurrentWorkspace(Some(workspace)) if workspace.id == SharedWorkspaceId::from("1")
        ));
        assert!(matches!(
            query_response_for_model(&model, QueryRequest::CurrentOutput),
            QueryResponse::CurrentOutput(Some(output)) if output.id == SharedOutputId::from("output-1")
        ));
        assert_eq!(
            query_response_for_model(&model, QueryRequest::WorkspaceNames),
            QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()])
        );
    }

    #[test]
    fn resolve_ipc_request_routes_queries_commands_and_session_responses() {
        let mut server = IpcServerState::new();
        let client_id = server.add_client();
        let mut commands = Vec::new();

        let query_response = resolve_ipc_request(
            &mut server,
            client_id,
            IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::WorkspaceNames))
                .with_request_id("req-query"),
            |_| QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]),
            |command| commands.push(command),
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
            |_| unreachable!("command request should not invoke query handler"),
            |command| commands.push(command),
        )
        .unwrap();

        assert_eq!(commands, vec![WmCommand::ReloadConfig]);
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
            |_| unreachable!("subscribe response should be produced by session"),
            |_| unreachable!("subscribe response should not invoke command handler"),
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

    #[test]
    fn ipc_socket_round_trips_query_response() {
        let socket_path = unique_socket_path("query-round-trip");
        let listener = bind_listener(&socket_path).unwrap();
        let mut client = connect(&socket_path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();
        let mut server = IpcServerState::new();
        let client_id = server.add_client();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::WorkspaceNames))
                .with_request_id("socket-query"),
        )
        .unwrap();

        let response = serve_ipc_stream_once(
            &mut server,
            client_id,
            &mut server_stream,
            |_| QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]),
            |_| unreachable!("query request should not execute command handler"),
        )
        .unwrap();

        assert_eq!(
            response,
            IpcEnvelope {
                request_id: Some("socket-query".into()),
                message: IpcServerMessage::Query(QueryResponse::WorkspaceNames(vec![
                    "1".into(),
                    "2".into(),
                ])),
            }
        );
        assert_eq!(recv_response(&client).unwrap(), response);

        drop(server_stream);
        drop(client);
        drop(listener);
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn ipc_socket_broadcasts_event_to_subscribed_client() {
        let socket_path = unique_socket_path("event-broadcast");
        let listener = bind_listener(&socket_path).unwrap();
        let mut client = connect(&socket_path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();
        let mut server = IpcServerState::new();
        let client_id = server.add_client();
        let mut writers = BTreeMap::from([(client_id, server_stream.try_clone().unwrap())]);

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Focus]))
                .with_request_id("socket-subscribe"),
        )
        .unwrap();

        let subscribe_response = serve_ipc_stream_once(
            &mut server,
            client_id,
            &mut server_stream,
            |_| unreachable!("subscribe should be handled by session"),
            |_| unreachable!("subscribe should not execute command handler"),
        )
        .unwrap();

        assert_eq!(recv_response(&client).unwrap(), subscribe_response);

        let stale_clients = broadcast_ipc_event_to_clients(
            &server,
            &mut writers,
            WmEvent::FocusChange {
                focused_window_id: Some(WindowId::from("win-1")),
                current_output_id: Some(spiders_core::OutputId::from("output-1")),
                current_workspace_id: Some(spiders_core::WorkspaceId::from("1")),
            },
        );

        assert!(stale_clients.is_empty());
        assert!(matches!(
            recv_response(&client).unwrap().message,
            IpcServerMessage::Event { event: WmEvent::FocusChange { focused_window_id: Some(window_id), .. }, .. }
                if window_id == WindowId::from("win-1")
        ));

        drop(server_stream);
        drop(client);
        drop(listener);
        let _ = std::fs::remove_file(socket_path);
    }

    fn serve_ipc_stream_once<QueryHandler, CommandHandler>(
        server: &mut IpcServerState,
        client_id: IpcClientId,
        stream: &mut UnixStream,
        query_handler: QueryHandler,
        command_handler: CommandHandler,
    ) -> Result<IpcResponse, WmIpcStreamError>
    where
        QueryHandler: FnMut(QueryRequest) -> QueryResponse,
        CommandHandler: FnMut(WmCommand),
    {
        let request = recv_request(stream)?;
        let response =
            resolve_ipc_request(server, client_id, request, query_handler, command_handler)?;
        send_response(stream, &response)?;
        Ok(response)
    }

    fn unique_socket_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("spiders-wm-{label}-{nanos}.sock"))
    }
}
