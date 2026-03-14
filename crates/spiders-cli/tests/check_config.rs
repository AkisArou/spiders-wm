use std::process::Command;
use std::{io::Write, os::unix::net::UnixListener};

use spiders_ipc::{encode_response_line, IpcEnvelope, IpcServerMessage};
use spiders_shared::api::QueryResponse;

fn cli_bin() -> String {
    env!("CARGO_BIN_EXE_spiders-cli").to_string()
}

fn bootstrap_fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bootstrap-events")
        .join(name)
}

fn runtime_fixture_paths() -> (std::path::PathBuf, std::path::PathBuf) {
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../spiders-config/tests/fixtures");
    (
        fixture_root.join("runtime"),
        fixture_root.join("project/config.ts"),
    )
}

fn write_runtime_config(name: &str) -> std::path::PathBuf {
    let (runtime_dir, _) = runtime_fixture_paths();
    let runtime_config = std::env::temp_dir().join(name);
    std::fs::write(
        &runtime_config,
        format!(
            r#"{{"layouts":[{{"name":"master-stack","module":"{}","stylesheet":"workspace {{ display: flex; }}"}}]}}"#,
            runtime_dir.join("layouts/master-stack.js").display()
        ),
    )
    .unwrap();
    runtime_config
}

#[test]
fn cli_reports_discovery_in_json_mode() {
    let output = Command::new(cli_bin())
        .arg("--json")
        .env("SPIDERS_WM_AUTHORED_CONFIG", "/tmp/authored.js")
        .env("SPIDERS_WM_RUNTIME_CONFIG", "/tmp/runtime.json")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["authored_config"], "/tmp/authored.js");
    assert_eq!(json["runtime_config"], "/tmp/runtime.json");
}

#[test]
fn cli_check_config_reports_validation_errors_in_json_mode() {
    let temp_dir = std::env::temp_dir();
    let runtime_config = temp_dir.join("spiders-cli-runtime-config.json");
    std::fs::write(
        &runtime_config,
        r#"{"layouts":[{"name":"missing","module":"layouts/missing.js","stylesheet":""}]}"#,
    )
    .unwrap();

    let output = Command::new(cli_bin())
        .arg("check-config")
        .arg("--json")
        .env("SPIDERS_WM_AUTHORED_CONFIG", "/tmp/authored.js")
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["phase"], "validation");
    assert!(json["errors"][0].as_str().unwrap().contains("missing"));

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_check_config_reports_success_in_json_mode_with_fixture_layout() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config = write_runtime_config("spiders-cli-runtime-success.json");

    let output = Command::new(cli_bin())
        .arg("check-config")
        .arg("--json")
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["layouts"], 1);

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_reports_json_diagnostics() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config = write_runtime_config("spiders-cli-bootstrap-trace.json");

    let output = Command::new(cli_bin())
        .arg("bootstrap-trace")
        .arg("--json")
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["controller_phase"], "pending");
    assert_eq!(json["active_seat"], "seat-0");
    assert_eq!(json["active_output"], "bootstrap-output");
    assert_eq!(json["current_workspace"], "bootstrap-workspace");
    assert_eq!(json["focused_window"], "bootstrap-window");
    assert_eq!(json["seat_names"][0], "seat-0");
    assert_eq!(json["output_ids"][0], "bootstrap-output");
    assert_eq!(json["startup"]["active_seat"], "seat-0");
    assert_eq!(json["startup"]["active_output"], "bootstrap-output");
    assert_eq!(json["seat_count"], 1);
    assert_eq!(json["output_count"], 1);
    assert_eq!(json["applied_events"], 0);

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_reports_script_failure_in_json_mode() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config = write_runtime_config("spiders-cli-bootstrap-trace-failure.json");
    let events_path = bootstrap_fixture("failure.json");
    let output = Command::new(cli_bin())
        .arg("bootstrap-trace")
        .arg("--json")
        .arg("--events")
        .arg(&events_path)
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["controller_phase"], "degraded");
    assert_eq!(json["applied_events"], 1);
    assert_eq!(
        json["failed_event"]["remove-output"]["output_id"],
        "missing-output"
    );
    assert_eq!(json["diagnostics"]["active_seat"], "seat-x");

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_reports_script_success_fixture() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config = write_runtime_config("spiders-cli-bootstrap-trace-success-script.json");
    let events_path = bootstrap_fixture("success.json");

    let output = Command::new(cli_bin())
        .arg("bootstrap-trace")
        .arg("--json")
        .arg("--events")
        .arg(&events_path)
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["controller_phase"], "running");
    assert_eq!(json["applied_events"], 4);
    assert_eq!(json["active_seat"], "seat-1");
    assert_eq!(json["seat_names"][1], "seat-1");
    assert!(json["surface_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "popup-1"));
    assert!(json["mapped_surface_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "window-w1"));

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_reports_transcript_success_fixture() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config =
        write_runtime_config("spiders-cli-bootstrap-trace-transcript-success.json");
    let transcript_path = bootstrap_fixture("transcript-success.json");

    let output = Command::new(cli_bin())
        .arg("bootstrap-trace")
        .arg("--json")
        .arg("--transcript")
        .arg(&transcript_path)
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["controller_phase"], "running");
    assert_eq!(json["applied_events"], 3);
    assert_eq!(json["active_seat"], "seat-1");
    assert_eq!(json["startup"]["active_seat"], "seat-1");
    assert!(json["surface_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "popup-1"));

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_accepts_transcript_fixture_via_events_flag() {
    let (_, authored_config) = runtime_fixture_paths();
    let runtime_config =
        write_runtime_config("spiders-cli-bootstrap-trace-transcript-via-events.json");
    let transcript_path = bootstrap_fixture("transcript-success.json");

    let output = Command::new(cli_bin())
        .arg("bootstrap-trace")
        .arg("--json")
        .arg("--events")
        .arg(&transcript_path)
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_RUNTIME_CONFIG", &runtime_config)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["controller_phase"], "running");
    assert_eq!(json["applied_events"], 3);
    assert_eq!(json["startup"]["active_seat"], "seat-1");

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_ipc_query_reports_socket_response_in_json_mode() {
    let socket_path = unique_socket_path("cli-ipc-query");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            reader.read_line(&mut request).unwrap();
            let line = encode_response_line(&IpcEnvelope::new(IpcServerMessage::Query(
                QueryResponse::TagNames(vec!["1".into(), "2".into()]),
            )))
            .unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("ipc-query")
        .arg("--json")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--query")
        .arg("tag-names")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["query"], "tag-names");
    assert_eq!(json["response"]["type"], "tag-names");
    assert_eq!(json["response"]["payload"][0], "1");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_action_reports_socket_response_in_json_mode() {
    let socket_path = unique_socket_path("cli-ipc-action");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            reader.read_line(&mut request).unwrap();
            let line =
                encode_response_line(&IpcEnvelope::new(IpcServerMessage::ActionAccepted)).unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("ipc-action")
        .arg("--json")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--action")
        .arg("reload-config")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["action"]["type"], "reload-config");
    assert_eq!(json["response_kind"], "action-accepted");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

fn unique_socket_path(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("spiders-cli-test-{label}-{nanos}.sock"))
}
