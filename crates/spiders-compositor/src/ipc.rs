use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;

use spiders_config::model::Config;
use spiders_ipc::{
    bind_listener, IpcCodecError, IpcServeError, IpcServerHandleResult, IpcServerState,
    IpcTransportError, UnknownClientError,
};
use spiders_shared::api::{QueryRequest, QueryResponse};
use spiders_shared::runtime::AuthoringLayoutRuntime;

use crate::actions::ActionError;
use crate::controller::CompositorController;

#[derive(Debug, thiserror::Error)]
pub enum CompositorIpcError {
    #[error(transparent)]
    Serve(#[from] IpcServeError),
    #[error(transparent)]
    Transport(#[from] IpcTransportError),
    #[error(transparent)]
    UnknownClient(#[from] UnknownClientError),
    #[error(transparent)]
    Action(#[from] ActionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcPumpReport {
    pub accepted_clients: usize,
    pub serviced_clients: usize,
    pub dropped_clients: usize,
}

#[derive(Debug)]
pub struct CompositorIpcHost {
    server: IpcServerState,
    listener: std::os::unix::net::UnixListener,
    clients: BTreeMap<spiders_ipc::IpcClientId, UnixStream>,
}

impl CompositorIpcHost {
    pub fn bind(path: impl AsRef<std::path::Path>) -> Result<Self, CompositorIpcError> {
        let listener = bind_listener(path)?;
        listener
            .set_nonblocking(true)
            .map_err(|error| CompositorIpcError::Transport(IpcTransportError::Io(error)))?;

        Ok(Self {
            server: IpcServerState::new(),
            listener,
            clients: BTreeMap::new(),
        })
    }

    pub fn listener(&self) -> &std::os::unix::net::UnixListener {
        &self.listener
    }

    pub fn server(&self) -> &IpcServerState {
        &self.server
    }

    pub fn add_client(&mut self) -> spiders_ipc::IpcClientId {
        self.server.add_client()
    }

    pub fn attach_client_stream(&mut self, stream: UnixStream) -> spiders_ipc::IpcClientId {
        let _ = stream.set_nonblocking(true);
        let client_id = self.server.add_client();
        self.clients.insert(client_id, stream);
        client_id
    }

    pub fn accept_client(&mut self) -> Result<spiders_ipc::IpcClientId, CompositorIpcError> {
        let (stream, _) = self
            .listener
            .accept()
            .map_err(|error| CompositorIpcError::Transport(IpcTransportError::Io(error)))?;
        Ok(self.attach_client_stream(stream))
    }

    pub fn remove_client(
        &mut self,
        client_id: spiders_ipc::IpcClientId,
    ) -> Option<spiders_ipc::IpcSession> {
        self.clients.remove(&client_id);
        self.server.remove_client(client_id)
    }

    pub fn serve_client_once<R>(
        &mut self,
        client_id: spiders_ipc::IpcClientId,
        controller: &mut CompositorController<R>,
    ) -> Result<spiders_ipc::IpcResponse, CompositorIpcError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        let request = {
            let stream = self
                .clients
                .get_mut(&client_id)
                .ok_or(UnknownClientError { client_id })?;
            spiders_ipc::recv_request(stream)?
        };

        let result = self.server.handle_request(client_id, request)?;
        let (response, emitted_events) = match result {
            IpcServerHandleResult::Response { response, .. } => (response, Vec::new()),
            IpcServerHandleResult::Query {
                client_id,
                request_id,
                query,
            } => (
                self.server.query_response(
                    client_id,
                    request_id,
                    query_response(controller, query),
                )?,
                Vec::new(),
            ),
            IpcServerHandleResult::Action {
                client_id,
                request_id,
                action,
            } => {
                let update = controller.apply_ipc_action(&action)?;
                (
                    self.server.action_accepted(client_id, request_id)?,
                    update.events,
                )
            }
        };

        {
            let stream = self
                .clients
                .get_mut(&client_id)
                .ok_or(UnknownClientError { client_id })?;
            spiders_ipc::send_response(stream, &response)?;
        }

        self.broadcast_events(emitted_events)?;

        Ok(response)
    }

    pub fn broadcast_event(
        &mut self,
        event: spiders_shared::api::CompositorEvent,
    ) -> Result<Vec<spiders_ipc::IpcClientId>, CompositorIpcError> {
        let responses = self.server.broadcast_event(event);
        let mut delivered = Vec::new();
        let mut dropped = Vec::new();

        for (client_id, response) in responses {
            match self.clients.get_mut(&client_id) {
                Some(stream) => match spiders_ipc::send_response(stream, &response) {
                    Ok(()) => delivered.push(client_id),
                    Err(_) => dropped.push(client_id),
                },
                None => dropped.push(client_id),
            }
        }

        for client_id in dropped {
            self.remove_client(client_id);
        }

        Ok(delivered)
    }

    pub fn broadcast_events(
        &mut self,
        events: impl IntoIterator<Item = spiders_shared::api::CompositorEvent>,
    ) -> Result<Vec<spiders_ipc::IpcClientId>, CompositorIpcError> {
        let mut delivered = Vec::new();

        for event in events {
            for client_id in self.broadcast_event(event)? {
                if !delivered.contains(&client_id) {
                    delivered.push(client_id);
                }
            }
        }

        Ok(delivered)
    }

    pub fn pump_once<R>(
        &mut self,
        controller: &mut CompositorController<R>,
    ) -> Result<IpcPumpReport, CompositorIpcError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        let mut accepted_clients = 0;

        loop {
            match self.accept_client() {
                Ok(_) => accepted_clients += 1,
                Err(CompositorIpcError::Transport(IpcTransportError::Io(error)))
                    if error.kind() == ErrorKind::WouldBlock =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }

        let client_ids: Vec<_> = self.clients.keys().copied().collect();
        let mut serviced_clients = 0;
        let mut dropped_clients = Vec::new();

        for client_id in client_ids {
            match self.serve_client_once(client_id, controller) {
                Ok(_) => serviced_clients += 1,
                Err(error) if ipc_error_is_would_block(&error) => {}
                Err(error) if ipc_error_is_empty_frame(&error) => dropped_clients.push(client_id),
                Err(CompositorIpcError::Transport(IpcTransportError::Codec(
                    IpcCodecError::InvalidJson(message),
                ))) => {
                    if self.send_error_to_client(client_id, message).is_err() {
                        dropped_clients.push(client_id);
                    }
                }
                Err(CompositorIpcError::Transport(IpcTransportError::Io(error)))
                    if matches!(
                        error.kind(),
                        ErrorKind::BrokenPipe
                            | ErrorKind::ConnectionReset
                            | ErrorKind::ConnectionAborted
                            | ErrorKind::UnexpectedEof
                    ) =>
                {
                    dropped_clients.push(client_id);
                }
                Err(CompositorIpcError::UnknownClient(_)) => dropped_clients.push(client_id),
                Err(error) => return Err(error),
            }
        }

        for client_id in &dropped_clients {
            self.remove_client(*client_id);
        }

        Ok(IpcPumpReport {
            accepted_clients,
            serviced_clients,
            dropped_clients: dropped_clients.len(),
        })
    }

    fn send_error_to_client(
        &mut self,
        client_id: spiders_ipc::IpcClientId,
        message: impl Into<String>,
    ) -> Result<(), CompositorIpcError> {
        let response = self.server.error_response(client_id, None, message)?;
        let stream = self
            .clients
            .get_mut(&client_id)
            .ok_or(UnknownClientError { client_id })?;
        spiders_ipc::send_response(stream, &response)?;
        Ok(())
    }
}

fn ipc_error_is_would_block(error: &CompositorIpcError) -> bool {
    matches!(
        error,
        CompositorIpcError::Transport(IpcTransportError::Io(io_error))
            if io_error.kind() == ErrorKind::WouldBlock
    )
}

fn ipc_error_is_empty_frame(error: &CompositorIpcError) -> bool {
    matches!(
        error,
        CompositorIpcError::Transport(IpcTransportError::Codec(IpcCodecError::EmptyFrame))
    )
}

fn query_response<R>(controller: &CompositorController<R>, query: QueryRequest) -> QueryResponse {
    let state = controller.state_snapshot();
    let focused_window = state.focused_window_id.as_ref().and_then(|window_id| {
        state
            .windows
            .iter()
            .find(|window| &window.id == window_id)
            .cloned()
    });

    match query {
        QueryRequest::State => QueryResponse::State(state),
        QueryRequest::FocusedWindow => QueryResponse::FocusedWindow(focused_window),
        QueryRequest::CurrentOutput => {
            QueryResponse::CurrentOutput(state.current_output().cloned())
        }
        QueryRequest::CurrentWorkspace => {
            QueryResponse::CurrentWorkspace(state.current_workspace().cloned())
        }
        QueryRequest::MonitorList => QueryResponse::MonitorList(state.outputs),
        QueryRequest::TagNames => QueryResponse::TagNames(state.tag_names),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::service::ConfigRuntimeService;
    use spiders_ipc::{
        recv_response, send_request, IpcClientMessage, IpcEnvelope, IpcServerMessage,
        IpcSubscriptionTopic,
    };
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::BoaPreparedLayoutRuntime;
    use spiders_shared::api::WmAction;
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn config() -> Config {
        Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        }
    }

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 800,
                logical_height: 600,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![WindowSnapshot {
                id: WindowId::from("w1"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("firefox".into()),
                title: Some("Firefox".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                floating: false,
                floating_rect: None,
                fullscreen: false,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            }],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into()],
        }
    }

    fn controller() -> CompositorController<BoaPreparedLayoutRuntime<RuntimeProjectLayoutSourceLoader>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-compositor-ipc-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaPreparedLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(runtime);

        CompositorController::initialize(service, config(), state()).unwrap()
    }

    #[test]
    fn ipc_host_serves_live_query_from_controller_state() {
        let path = unique_socket_path("query");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        let client_id = host.accept_client().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::TagNames)),
        )
        .unwrap();
        let response = host.serve_client_once(client_id, &mut controller).unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(response, decoded);
        assert!(matches!(
            decoded.message,
            spiders_ipc::IpcServerMessage::Query(QueryResponse::TagNames(_))
        ));

        drop(client);
        let _ = host.remove_client(client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_serves_live_action_against_controller_state() {
        let path = unique_socket_path("action");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        let client_id = host.accept_client().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::ToggleFloating)),
        )
        .unwrap();
        let response = host.serve_client_once(client_id, &mut controller).unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(response, decoded);
        assert!(matches!(
            decoded.message,
            spiders_ipc::IpcServerMessage::ActionAccepted
        ));
        assert!(
            controller
                .state_snapshot()
                .windows
                .iter()
                .find(|window| window.id == WindowId::from("w1"))
                .unwrap()
                .floating
        );

        drop(client);
        let _ = host.remove_client(client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_serves_workspace_activate_action_against_controller_state() {
        let path = unique_socket_path("workspace-activate");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();
        controller
            .apply_ipc_action(&WmAction::AssignWorkspace {
                workspace_id: WorkspaceId::from("ws-1"),
                output_id: OutputId::from("out-1"),
            })
            .unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        let client_id = host.accept_client().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::ActivateWorkspace {
                workspace_id: WorkspaceId::from("ws-1"),
            })),
        )
        .unwrap();
        let response = host.serve_client_once(client_id, &mut controller).unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(response, decoded);
        assert!(matches!(decoded.message, IpcServerMessage::ActionAccepted));
        assert_eq!(
            controller.state_snapshot().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );

        drop(client);
        let _ = host.remove_client(client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_serves_workspace_assign_action_against_controller_state() {
        let path = unique_socket_path("workspace-assign");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-compositor-ipc-assign-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();
        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaPreparedLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(runtime);
        let mut startup_state = state();
        startup_state.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 1024,
            logical_height: 768,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut controller =
            CompositorController::initialize(service, config(), startup_state).unwrap();

        let mut client = UnixStream::connect(&path).unwrap();
        let client_id = host.accept_client().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::AssignWorkspace {
                workspace_id: WorkspaceId::from("ws-1"),
                output_id: OutputId::from("out-2"),
            })),
        )
        .unwrap();
        let response = host.serve_client_once(client_id, &mut controller).unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(response, decoded);
        assert!(matches!(decoded.message, IpcServerMessage::ActionAccepted));
        assert_eq!(
            controller
                .state_snapshot()
                .workspace_by_id(&WorkspaceId::from("ws-1"))
                .unwrap()
                .output_id,
            Some(OutputId::from("out-2"))
        );

        drop(client);
        let _ = host.remove_client(client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_broadcasts_subscribed_action_events_to_other_clients() {
        let path = unique_socket_path("broadcast-action-events");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut subscriber = UnixStream::connect(&path).unwrap();
        let subscriber_id = host.accept_client().unwrap();
        send_request(
            &mut subscriber,
            &IpcEnvelope::new(IpcClientMessage::subscribe([
                IpcSubscriptionTopic::Windows,
                IpcSubscriptionTopic::Focus,
            ])),
        )
        .unwrap();
        let subscribed = host
            .serve_client_once(subscriber_id, &mut controller)
            .unwrap();
        assert!(matches!(
            subscribed.message,
            IpcServerMessage::Subscribed { .. }
        ));
        let _ = recv_response(&subscriber).unwrap();

        let mut actor = UnixStream::connect(&path).unwrap();
        let actor_id = host.accept_client().unwrap();
        send_request(
            &mut actor,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::ToggleFloating)),
        )
        .unwrap();
        let action_response = host.serve_client_once(actor_id, &mut controller).unwrap();
        assert!(matches!(
            action_response.message,
            IpcServerMessage::ActionAccepted
        ));
        let _ = recv_response(&actor).unwrap();

        let event = recv_response(&subscriber).unwrap();
        assert!(matches!(event.message, IpcServerMessage::Event { .. }));

        drop(actor);
        drop(subscriber);
        let _ = host.remove_client(actor_id);
        let _ = host.remove_client(subscriber_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_broadcast_event_targets_matching_subscribers() {
        let path = unique_socket_path("broadcast-manual-event");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut layout_client = UnixStream::connect(&path).unwrap();
        let layout_client_id = host.accept_client().unwrap();
        send_request(
            &mut layout_client,
            &IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Layout])),
        )
        .unwrap();
        let _ = host
            .serve_client_once(layout_client_id, &mut controller)
            .unwrap();
        let _ = recv_response(&layout_client).unwrap();

        let mut focus_client = UnixStream::connect(&path).unwrap();
        let focus_client_id = host.accept_client().unwrap();
        send_request(
            &mut focus_client,
            &IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Focus])),
        )
        .unwrap();
        let _ = host
            .serve_client_once(focus_client_id, &mut controller)
            .unwrap();
        let _ = recv_response(&focus_client).unwrap();

        let delivered = host
            .broadcast_event(spiders_shared::api::CompositorEvent::LayoutChange {
                workspace_id: None,
                layout: None,
            })
            .unwrap();

        assert_eq!(delivered, vec![layout_client_id]);
        let event = recv_response(&layout_client).unwrap();
        assert!(matches!(event.message, IpcServerMessage::Event { .. }));

        drop(layout_client);
        drop(focus_client);
        let _ = host.remove_client(layout_client_id);
        let _ = host.remove_client(focus_client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_pump_once_accepts_and_services_pending_clients() {
        let path = unique_socket_path("pump-once");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::TagNames)),
        )
        .unwrap();

        let report = host.pump_once(&mut controller).unwrap();
        let response = recv_response(&client).unwrap();

        assert_eq!(report.accepted_clients, 1);
        assert_eq!(report.serviced_clients, 1);
        assert_eq!(report.dropped_clients, 0);
        assert!(matches!(
            response.message,
            IpcServerMessage::Query(QueryResponse::TagNames(_))
        ));

        drop(client);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_pump_once_drops_empty_frame_clients() {
        let path = unique_socket_path("pump-drop-empty");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        client.write_all(b"\n").unwrap();

        let report = host.pump_once(&mut controller).unwrap();

        assert_eq!(report.accepted_clients, 1);
        assert_eq!(report.serviced_clients, 0);
        assert_eq!(report.dropped_clients, 1);

        drop(client);
        std::fs::remove_file(path).unwrap();
    }

    fn unique_socket_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-compositor-ipc-{label}-{nanos}.sock"))
    }
}
