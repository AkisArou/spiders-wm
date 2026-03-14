pub mod codec;
pub mod protocol;
pub mod server;
pub mod session;
pub mod transport;

pub use codec::{
    decode_request_line, decode_response_line, encode_request_line, encode_response_line,
    IpcCodecError,
};
pub use protocol::{
    infer_topics, normalize_topics, subscription_matches_event, subscription_matches_topics,
    IpcClientMessage, IpcEnvelope, IpcRequest, IpcResponse, IpcServerMessage, IpcSubscriptionTopic,
};
pub use server::{
    IpcClientId, IpcServeError, IpcServerHandleResult, IpcServerState, UnknownClientError,
};
pub use session::{IpcSession, IpcSessionHandleResult};
pub use transport::{
    bind_listener, connect, recv_request, recv_response, send_request, send_response,
    IpcTransportError,
};

pub fn crate_ready() -> bool {
    true
}
