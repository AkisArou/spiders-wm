pub mod codec;
pub mod protocol;
pub mod server;
pub mod session;

pub use codec::{
    decode_request_line, decode_response_line, encode_request_line, encode_response_line,
    IpcCodecError,
};
pub use protocol::{
    infer_topics, normalize_topics, subscription_matches_event, subscription_matches_topics,
    IpcClientMessage, IpcEnvelope, IpcRequest, IpcResponse, IpcServerMessage, IpcSubscriptionTopic,
};
pub use server::{IpcClientId, IpcServerHandleResult, IpcServerState, UnknownClientError};
pub use session::{IpcSession, IpcSessionHandleResult};

pub fn crate_ready() -> bool {
    true
}
