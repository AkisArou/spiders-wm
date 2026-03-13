use serde::{Deserialize, Serialize};

use spiders_shared::api::{CompositorEvent, QueryRequest, QueryResponse, WmAction};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum IpcRequest {
    Query(QueryRequest),
    Action(WmAction),
    Subscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum IpcResponse {
    Query(QueryResponse),
    Event(CompositorEvent),
    ActionAccepted,
    Error { message: String },
}
