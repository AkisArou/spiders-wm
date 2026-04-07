use serde::Serialize;
use spiders_core::command::WmCommand;
use spiders_core::event::WmEvent;
use spiders_core::query::{QueryRequest, QueryResponse};
use spiders_core::runtime::runtime_error::RuntimeRefreshSummary;
use spiders_ipc::DebugDumpKind;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DiscoveryReport {
    pub status: &'static str,
    pub authored_config: String,
    pub prepared_config: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SuccessCheckReport {
    pub status: &'static str,
    pub layouts: usize,
    pub prepared_config: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_config_update: Option<RuntimeRefreshSummary>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BuildConfigReport {
    pub status: &'static str,
    pub authored_config: String,
    pub prepared_config: String,
    pub layouts: usize,
    pub prepared_config_update: RuntimeRefreshSummary,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ErrorReport {
    pub status: &'static str,
    pub phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct IpcSmokeReport {
    pub status: &'static str,
    pub client_id: u64,
    pub request_kind: &'static str,
    pub response_kind: &'static str,
    pub request_line: String,
    pub response_line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_line: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct IpcQueryReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub query: QueryRequest,
    pub response: QueryResponse,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct IpcCommandReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub command: WmCommand,
    pub response_kind: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct IpcDebugReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub dump_kind: DebugDumpKind,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct IpcMonitorReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub topics: Vec<String>,
    pub subscribed_topics: Vec<String>,
    pub events: Vec<WmEvent>,
}

pub fn emit<T: Serialize>(mode: OutputMode, report: &T, text: impl FnOnce() -> String) {
    match mode {
        OutputMode::Text => println!("{}", text()),
        OutputMode::Json => println!("{}", serde_json::to_string(report).unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_report_serializes_without_empty_optional_fields() {
        let report = ErrorReport {
            status: "error",
            phase: "load",
            prepared_config: None,
            errors: None,
            message: Some("boom".into()),
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["status"], "error");
        assert_eq!(json["phase"], "load");
        assert!(json.get("errors").is_none());
        assert!(json.get("prepared_config").is_none());
    }
}
