pub mod codec;
pub mod handler;
pub mod protocol;
pub mod resolver;
pub mod server;
pub mod session;

pub use codec::{
    IpcCodecError, decode_request_line, decode_response_line, encode_request_line,
    encode_response_line,
};
pub use handler::IpcHandler;
pub use protocol::{
    DebugDumpKind, DebugRequest, DebugResponse, IpcClientMessage, IpcEnvelope, IpcRequest,
    IpcResponse, IpcServerMessage, IpcSubscriptionTopic, infer_topics, normalize_topics,
    subscription_matches_event, subscription_matches_topics,
};
pub use resolver::ResolveIpcRequestError;
pub use resolver::resolve_ipc_request;
pub use server::{IpcClientId, IpcServerHandleResult, IpcServerState, UnknownClientError};
pub use session::{IpcSession, IpcSessionHandleResult};
