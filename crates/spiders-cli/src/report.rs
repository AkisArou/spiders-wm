use serde::Serialize;
use spiders_runtime::{BootstrapDiagnostics, BootstrapEvent, ControllerPhase, StartupRegistration};
use spiders_shared::api::{CompositorEvent, QueryRequest, QueryResponse, WmAction};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DiscoveryReport {
    pub status: &'static str,
    pub runtime_ready: bool,
    pub authored_config: String,
    pub runtime_config: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SuccessCheckReport {
    pub status: &'static str,
    pub runtime_ready: bool,
    pub layouts: usize,
    pub runtime_config: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ErrorReport {
    pub status: &'static str,
    pub phase: &'static str,
    pub runtime_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BootstrapReport {
    pub status: &'static str,
    pub runtime_ready: bool,
    pub authored_config: String,
    pub runtime_config: String,
    pub controller_phase: ControllerPhase,
    pub active_seat: Option<String>,
    pub active_output: Option<String>,
    pub current_workspace: Option<String>,
    pub focused_window: Option<String>,
    pub seat_names: Vec<String>,
    pub output_ids: Vec<String>,
    pub surface_ids: Vec<String>,
    pub mapped_surface_ids: Vec<String>,
    pub seat_count: usize,
    pub output_count: usize,
    pub surface_count: usize,
    pub mapped_surface_count: usize,
    pub applied_events: usize,
    pub startup: StartupRegistration,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BootstrapFailureReport {
    pub status: &'static str,
    pub runtime_ready: bool,
    pub authored_config: String,
    pub runtime_config: String,
    pub controller_phase: ControllerPhase,
    pub error: String,
    pub failed_event: Option<BootstrapEvent>,
    pub applied_events: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<BootstrapDiagnostics>,
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

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct IpcQueryReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub query: QueryRequest,
    pub response: QueryResponse,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct IpcActionReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub action: WmAction,
    pub response_kind: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct IpcMonitorReport {
    pub status: &'static str,
    pub socket_path: String,
    pub request_id: String,
    pub topics: Vec<String>,
    pub subscribed_topics: Vec<String>,
    pub events: Vec<CompositorEvent>,
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
            runtime_ready: true,
            runtime_config: None,
            errors: None,
            message: Some("boom".into()),
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["status"], "error");
        assert_eq!(json["phase"], "load");
        assert!(json.get("errors").is_none());
        assert!(json.get("runtime_config").is_none());
    }

    #[test]
    fn bootstrap_report_serializes_expected_fields() {
        let report = BootstrapReport {
            status: "ok",
            runtime_ready: true,
            authored_config: "/tmp/authored.js".into(),
            runtime_config: "/tmp/runtime.json".into(),
            controller_phase: ControllerPhase::Running,
            active_seat: Some("seat-0".into()),
            active_output: Some("out-1".into()),
            current_workspace: Some("ws-1".into()),
            focused_window: Some("w1".into()),
            seat_names: vec!["seat-0".into()],
            output_ids: vec!["out-1".into()],
            surface_ids: vec!["window-w1".into()],
            mapped_surface_ids: vec!["window-w1".into()],
            seat_count: 1,
            output_count: 1,
            surface_count: 0,
            mapped_surface_count: 0,
            applied_events: 0,
            startup: StartupRegistration {
                seats: vec!["seat-0".into()],
                outputs: vec![spiders_shared::ids::OutputId::from("out-1")],
                active_seat: Some("seat-0".into()),
                active_output: Some(spiders_shared::ids::OutputId::from("out-1")),
            },
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["status"], "ok");
        assert_eq!(json["controller_phase"], "running");
        assert_eq!(json["active_seat"], "seat-0");
        assert_eq!(json["current_workspace"], "ws-1");
        assert_eq!(json["focused_window"], "w1");
        assert_eq!(json["seat_names"][0], "seat-0");
        assert_eq!(json["startup"]["active_seat"], "seat-0");
    }

    #[test]
    fn bootstrap_failure_report_serializes_failed_event() {
        let report = BootstrapFailureReport {
            status: "error",
            runtime_ready: true,
            authored_config: "/tmp/authored.js".into(),
            runtime_config: "/tmp/runtime.json".into(),
            controller_phase: ControllerPhase::Degraded,
            error: "boom".into(),
            failed_event: Some(BootstrapEvent::RemoveOutput {
                output_id: spiders_shared::ids::OutputId::from("out-9"),
            }),
            applied_events: 1,
            diagnostics: None,
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["status"], "error");
        assert_eq!(json["controller_phase"], "degraded");
        assert_eq!(json["failed_event"]["remove-output"]["output_id"], "out-9");
    }
}
