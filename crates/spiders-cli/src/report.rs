use serde::Serialize;

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
    pub active_seat: Option<String>,
    pub active_output: Option<String>,
    pub seat_count: usize,
    pub output_count: usize,
    pub surface_count: usize,
    pub mapped_surface_count: usize,
    pub applied_events: usize,
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
}
