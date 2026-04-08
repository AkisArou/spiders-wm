use std::collections::BTreeMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use spiders_core::event::WmEvent;
use spiders_ipc_core::{
    IpcClientId, IpcHandler, IpcResponse, IpcServerState, ResolveIpcRequestError,
    UnknownClientError, resolve_ipc_request,
};
use tracing::{debug, warn};

use crate::{IpcTransportError, bind_listener, recv_request, send_response};

#[derive(Debug, Default)]
pub struct NativeIpcState {
    pub server: IpcServerState,
    pub clients: BTreeMap<IpcClientId, UnixStream>,
    pub socket_path: Option<PathBuf>,
}

impl NativeIpcState {
    pub fn init_socket_path(&mut self, app_socket_name: &str) -> Option<PathBuf> {
        if std::env::var_os("SPIDERS_WM_DISABLE_IPC").is_some() {
            return None;
        }

        let path = configured_ipc_socket_path(app_socket_name);
        self.socket_path = Some(path.clone());
        Some(path)
    }

    pub fn register_client_stream(&mut self, stream: UnixStream) -> std::io::Result<IpcClientId> {
        let client_id = self.server.add_client();
        let writer = stream.try_clone()?;
        self.clients.insert(client_id, writer);
        Ok(client_id)
    }

    pub fn remove_client(&mut self, client_id: IpcClientId) {
        self.clients.remove(&client_id);
        self.server.remove_client(client_id);
    }

    pub fn broadcast_event(&mut self, event: WmEvent) {
        let stale_clients = broadcast_ipc_event_to_clients(&self.server, &mut self.clients, event);
        for client_id in stale_clients {
            self.remove_client(client_id);
        }
    }
}

pub fn bind_native_ipc_listener(socket_path: &Path) -> Result<UnixListener, IpcTransportError> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = bind_listener(socket_path)?;
    listener.set_nonblocking(true)?;
    debug!(path = %socket_path.display(), "bound native IPC socket");
    Ok(listener)
}

pub fn accept_pending_ipc_clients(
    ipc: &mut NativeIpcState,
    listener: &UnixListener,
) -> Vec<(IpcClientId, UnixStream)> {
    let mut accepted = Vec::new();

    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    warn!(%error, "failed to set IPC client stream nonblocking");
                    continue;
                }

                match ipc.register_client_stream(stream.try_clone().expect("clone client stream")) {
                    Ok(client_id) => accepted.push((client_id, stream)),
                    Err(error) => warn!(%error, "failed to register IPC client stream"),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => {
                warn!(%error, "failed to accept IPC client stream");
                break;
            }
        }
    }

    accepted
}

pub fn broadcast_ipc_event_to_clients(
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

pub fn serve_ipc_client_once<H>(
    ipc: &mut NativeIpcState,
    client_id: IpcClientId,
    handler: &mut H,
) -> Result<IpcResponse, NativeIpcServeError<H::Error>>
where
    H: IpcHandler,
    H::Error: std::error::Error + Send + Sync + 'static,
{
    let request = {
        let stream = ipc.clients.get_mut(&client_id).ok_or(UnknownClientError { client_id })?;
        recv_request(stream)?
    };
    let response = resolve_ipc_request(&mut ipc.server, client_id, request, handler)?;
    let stream = ipc.clients.get_mut(&client_id).ok_or(UnknownClientError { client_id })?;
    send_response(stream, &response)?;
    Ok(response)
}

#[derive(Debug, thiserror::Error)]
pub enum NativeIpcServeError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    #[error(transparent)]
    Transport(#[from] IpcTransportError),
    #[error(transparent)]
    UnknownClient(#[from] UnknownClientError),
    #[error("handler error: {0}")]
    Handler(E),
}

impl<E> From<ResolveIpcRequestError<E>> for NativeIpcServeError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: ResolveIpcRequestError<E>) -> Self {
        match value {
            ResolveIpcRequestError::UnknownClient(error) => Self::UnknownClient(error),
            ResolveIpcRequestError::Handler(error) => Self::Handler(error),
        }
    }
}

pub fn stream_io_error(error: IpcTransportError) -> std::io::Error {
    match error {
        IpcTransportError::Io(error) => error,
        IpcTransportError::Codec(error) => {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error)
        }
    }
}

pub fn default_ipc_socket_path(app_socket_name: &str) -> PathBuf {
    let base =
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(format!("{app_socket_name}-{}.sock", std::process::id()))
}

fn configured_ipc_socket_path(app_socket_name: &str) -> PathBuf {
    std::env::var_os("SPIDERS_WM_IPC_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_ipc_socket_path(app_socket_name))
}
