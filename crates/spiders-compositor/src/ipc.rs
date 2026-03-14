use std::os::unix::net::UnixStream;

use spiders_config::runtime::LayoutRuntime;
use spiders_ipc::{
    bind_listener, IpcServeError, IpcServerHandleResult, IpcServerState, IpcTransportError,
    UnknownClientError,
};
use spiders_shared::api::{QueryRequest, QueryResponse};

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

#[derive(Debug)]
pub struct CompositorIpcHost {
    server: IpcServerState,
    listener: std::os::unix::net::UnixListener,
}

impl CompositorIpcHost {
    pub fn bind(path: impl AsRef<std::path::Path>) -> Result<Self, CompositorIpcError> {
        Ok(Self {
            server: IpcServerState::new(),
            listener: bind_listener(path)?,
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

    pub fn remove_client(
        &mut self,
        client_id: spiders_ipc::IpcClientId,
    ) -> Option<spiders_ipc::IpcSession> {
        self.server.remove_client(client_id)
    }

    pub fn serve_client_once<L, R>(
        &mut self,
        client_id: spiders_ipc::IpcClientId,
        stream: &mut UnixStream,
        controller: &mut CompositorController<L, R>,
    ) -> Result<spiders_ipc::IpcResponse, CompositorIpcError>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: LayoutRuntime,
    {
        let request = spiders_ipc::recv_request(stream)?;
        let result = self.server.handle_request(client_id, request)?;
        let response = match result {
            IpcServerHandleResult::Response { response, .. } => response,
            IpcServerHandleResult::Query {
                client_id,
                request_id,
                query,
            } => self.server.query_response(
                client_id,
                request_id,
                query_response(controller, query),
            )?,
            IpcServerHandleResult::Action {
                client_id,
                request_id,
                action,
            } => {
                controller.apply_ipc_action(&action)?;
                self.server.action_accepted(client_id, request_id)?
            }
        };

        spiders_ipc::send_response(stream, &response)?;

        Ok(response)
    }
}

fn query_response<L, R>(
    controller: &CompositorController<L, R>,
    query: QueryRequest,
) -> QueryResponse {
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
    use std::os::unix::net::UnixStream;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_config::runtime::BoaLayoutRuntime;
    use spiders_config::service::ConfigRuntimeService;
    use spiders_ipc::{recv_response, send_request, IpcClientMessage, IpcEnvelope};
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

    fn controller() -> CompositorController<
        RuntimeProjectLayoutSourceLoader,
        BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
    > {
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
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service = ConfigRuntimeService::new(loader, runtime);

        CompositorController::initialize(service, config(), state()).unwrap()
    }

    #[test]
    fn ipc_host_serves_live_query_from_controller_state() {
        let path = unique_socket_path("query");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let client_id = host.add_client();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server_stream, _) = host.listener().accept().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::TagNames)),
        )
        .unwrap();
        let response = host
            .serve_client_once(client_id, &mut server_stream, &mut controller)
            .unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(response, decoded);
        assert!(matches!(
            decoded.message,
            spiders_ipc::IpcServerMessage::Query(QueryResponse::TagNames(_))
        ));

        drop(server_stream);
        drop(client);
        let _ = host.remove_client(client_id);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ipc_host_serves_live_action_against_controller_state() {
        let path = unique_socket_path("action");
        let mut host = CompositorIpcHost::bind(&path).unwrap();
        let client_id = host.add_client();
        let mut controller = controller();

        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server_stream, _) = host.listener().accept().unwrap();

        send_request(
            &mut client,
            &IpcEnvelope::new(IpcClientMessage::Action(WmAction::ToggleFloating)),
        )
        .unwrap();
        let response = host
            .serve_client_once(client_id, &mut server_stream, &mut controller)
            .unwrap();
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

        drop(server_stream);
        drop(client);
        let _ = host.remove_client(client_id);
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
