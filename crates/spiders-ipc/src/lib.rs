pub mod protocol;

pub use protocol::{
    infer_topics, normalize_topics, subscription_matches_event, subscription_matches_topics,
    IpcClientMessage, IpcEnvelope, IpcRequest, IpcResponse, IpcServerMessage, IpcSubscriptionTopic,
};

pub fn crate_ready() -> bool {
    true
}
