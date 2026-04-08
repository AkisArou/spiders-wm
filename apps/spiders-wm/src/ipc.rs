#![allow(dead_code)]

use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use spiders_core::event::WmEvent;
use spiders_core::query::{QueryRequest, QueryResponse, query_response_for_model};
use spiders_ipc::{DebugRequest, IpcClientId, IpcCodecError, IpcResponse, IpcServerState};
use spiders_ipc_core::{IpcHandler, resolve_ipc_request};
use spiders_ipc_native::{
    IpcTransportError, NativeIpcServeError, accept_pending_ipc_clients, bind_native_ipc_listener,
    default_ipc_socket_path, serve_ipc_client_once,
};
use tracing::warn;

use crate::state::SpidersWm;

impl SpidersWm {
    pub fn broadcast_runtime_events(&mut self, events: impl IntoIterator<Item = WmEvent>) {
        for event in events {
            self.broadcast_ipc_event(event);
        }
    }

    pub fn ipc_add_client(&mut self) -> IpcClientId {
        self.ipc.server.add_client()
    }

    pub fn ipc_remove_client(&mut self, client_id: IpcClientId) {
        self.ipc.remove_client(client_id);
    }

    pub fn handle_ipc_request(
        &mut self,
        client_id: IpcClientId,
        request: spiders_ipc::IpcRequest,
    ) -> Result<IpcResponse, spiders_ipc_core::ResolveIpcRequestError<std::io::Error>> {
        let mut server = std::mem::take(&mut self.ipc.server);
        let result = {
            let mut handler = WaylandIpcHandler { wm: self };
            resolve_ipc_request(&mut server, client_id, request, &mut handler)
        };
        self.ipc.server = server;
        result
    }

    pub fn query_ipc(&self, query: QueryRequest) -> QueryResponse {
        query_response_for_model(&self.model, query)
    }

    pub fn register_ipc_client_stream(
        &mut self,
        client_id: IpcClientId,
        stream: UnixStream,
    ) -> std::io::Result<()> {
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
            Err(NativeIpcServeError::Transport(IpcTransportError::Io(error)))
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
            Err(NativeIpcServeError::Transport(IpcTransportError::Codec(
                IpcCodecError::EmptyFrame,
            ))) => {
                self.ipc_remove_client(client_id);
                Ok(PostAction::Remove)
            }
            Err(NativeIpcServeError::UnknownClient(_)) => Ok(PostAction::Remove),
            Err(NativeIpcServeError::Transport(IpcTransportError::Codec(error))) => {
                warn!(client_id, %error, "discarding malformed IPC request");
                let response = self
                    .ipc
                    .server
                    .error_response(client_id, None, error.to_string())
                    .map_err(std::io::Error::other)?;
                let Some(stream) = self.ipc.clients.get_mut(&client_id) else {
                    self.ipc_remove_client(client_id);
                    return Ok(PostAction::Remove);
                };
                match spiders_ipc_native::send_response(stream, &response).map_err(stream_io_error)
                {
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
            Err(NativeIpcServeError::Transport(IpcTransportError::Io(error))) => Err(error),
            Err(NativeIpcServeError::Handler(error)) => Err(std::io::Error::other(error)),
        }
    }

    pub fn broadcast_ipc_event(&mut self, event: WmEvent) {
        self.ipc.broadcast_event(event);
    }

    pub fn emit_config_reloaded(&mut self) {
        self.broadcast_ipc_event(WmEvent::ConfigReloaded);
    }
}

pub(crate) fn init_ipc_listener(event_loop: &mut EventLoop<'static, SpidersWm>) -> Option<PathBuf> {
    if std::env::var_os("SPIDERS_WM_DISABLE_IPC").is_some() {
        return None;
    }
    let socket_path = std::env::var_os("SPIDERS_WM_IPC_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_ipc_socket_path("spiders-wm"));
    let listener = match bind_native_ipc_listener(&socket_path) {
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
                for (client_id, stream) in accept_pending_ipc_clients(&mut state.ipc, listener) {
                    if let Err(error) = state.register_ipc_client_stream(client_id, stream) {
                        warn!(%error, "failed to register IPC client stream");
                        state.ipc.remove_client(client_id);
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("failed to register IPC listener socket");

    Some(socket_path)
}

pub(crate) fn serve_ipc_client_stream(
    state: &mut SpidersWm,
    client_id: IpcClientId,
) -> Result<IpcResponse, NativeIpcServeError<std::io::Error>> {
    let mut ipc = std::mem::take(&mut state.ipc);
    let result = {
        let mut handler = WaylandIpcHandler { wm: state };
        serve_ipc_client_once(&mut ipc, client_id, &mut handler)
    };
    state.ipc = ipc;
    result
}

fn stream_io_error(error: IpcTransportError) -> std::io::Error {
    match error {
        IpcTransportError::Io(error) => error,
        IpcTransportError::Codec(error) => {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error)
        }
    }
}

struct WaylandIpcHandler<'a> {
    wm: &'a mut SpidersWm,
}

impl IpcHandler for WaylandIpcHandler<'_> {
    type Error = std::io::Error;

    fn handle_query(&mut self, query: QueryRequest) -> Result<QueryResponse, Self::Error> {
        Ok(self.wm.query_ipc(query))
    }

    fn handle_command(
        &mut self,
        command: spiders_core::command::WmCommand,
    ) -> Result<(), Self::Error> {
        self.wm.execute_wm_command(command);
        Ok(())
    }

    fn handle_debug(
        &mut self,
        request: DebugRequest,
    ) -> Result<spiders_ipc::DebugResponse, Self::Error> {
        match request {
            DebugRequest::Dump { kind } => {
                self.wm.handle_debug_dump(kind).map_err(std::io::Error::other)
            }
        }
    }
}

#[cfg(test)]
fn serve_ipc_stream_once<H>(
    server: &mut IpcServerState,
    client_id: IpcClientId,
    stream: &mut UnixStream,
    handler: &mut H,
) -> Result<IpcResponse, NativeIpcServeError<H::Error>>
where
    H: IpcHandler,
    H::Error: std::error::Error + Send + Sync + 'static,
{
    let mut ipc = spiders_ipc_native::NativeIpcState::default();
    ipc.server = std::mem::take(server);
    ipc.clients.insert(client_id, stream.try_clone().map_err(IpcTransportError::from)?);
    let response = serve_ipc_client_once(&mut ipc, client_id, handler)?;
    *server = ipc.server;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::net::UnixStream;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_core::command::WmCommand;
    use spiders_core::query::{
        QueryRequest, QueryResponse, query_response_for_model, state_snapshot_for_model,
    };
    use spiders_core::types::WindowMode;
    use spiders_core::wm::WmModel;
    use spiders_core::{OutputId as SharedOutputId, WindowId, WorkspaceId as SharedWorkspaceId};
    use spiders_core::{OutputId, WorkspaceId};
    use spiders_ipc::DebugResponse;
    use spiders_ipc::{IpcClientMessage, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic};
    use spiders_ipc_core::IpcHandler;
    use spiders_ipc_core::IpcServerState;
    use spiders_ipc_native::{
        bind_listener, broadcast_ipc_event_to_clients, connect, recv_response, send_request,
    };

    use super::*;

    struct TestHandler {
        commands: Vec<WmCommand>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self { commands: Vec::new() }
        }
    }

    impl IpcHandler for TestHandler {
        type Error = std::io::Error;

        fn handle_query(&mut self, _query: QueryRequest) -> Result<QueryResponse, Self::Error> {
            Ok(QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]))
        }

        fn handle_command(&mut self, command: WmCommand) -> Result<(), Self::Error> {
            self.commands.push(command);
            Ok(())
        }

        fn handle_debug(
            &mut self,
            _request: DebugRequest,
        ) -> Result<spiders_ipc::DebugResponse, Self::Error> {
            Ok(DebugResponse::DumpWritten { kind: spiders_ipc::DebugDumpKind::WmState, path: None })
        }
    }

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

    #[test]
    fn ipc_socket_round_trips_query_response() {
        let socket_path = unique_socket_path("query-round-trip");
        let listener = bind_listener(&socket_path).unwrap();
        let mut client = connect(&socket_path).unwrap();
        let (mut server_stream, _) = listener.accept().unwrap();
        let mut server = IpcServerState::new();
        let client_id = server.add_client();
        let mut handler = TestHandler::new();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::WorkspaceNames))
                .with_request_id("socket-query"),
        )
        .unwrap();

        let response =
            serve_ipc_stream_once(&mut server, client_id, &mut server_stream, &mut handler)
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
        let mut handler = TestHandler::new();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Focus]))
                .with_request_id("socket-subscribe"),
        )
        .unwrap();

        let subscribe_response =
            serve_ipc_stream_once(&mut server, client_id, &mut server_stream, &mut handler)
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

    fn serve_ipc_stream_once<H>(
        server: &mut IpcServerState,
        client_id: IpcClientId,
        stream: &mut UnixStream,
        handler: &mut H,
    ) -> Result<IpcResponse, NativeIpcServeError<H::Error>>
    where
        H: IpcHandler,
        H::Error: std::error::Error + Send + Sync + 'static,
    {
        super::serve_ipc_stream_once(server, client_id, stream, handler)
    }

    fn unique_socket_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("spiders-wm-{label}-{nanos}.sock"))
    }
}
