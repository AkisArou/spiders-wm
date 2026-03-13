use std::process::Command;

fn cli_bin() -> String {
    env!("CARGO_BIN_EXE_spiders-cli").to_string()
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
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../spiders-config/tests/fixtures");
    let runtime_config = std::env::temp_dir().join("spiders-cli-runtime-success.json");
    let runtime_dir = fixture_root.join("runtime");
    let authored_config = fixture_root.join("project/config.ts");
    std::fs::write(
        &runtime_config,
        format!(
            r#"{{"layouts":[{{"name":"master-stack","module":"{}","stylesheet":"workspace {{ display: flex; }}"}}]}}"#,
            runtime_dir.join("layouts/master-stack.js").display()
        ),
    )
    .unwrap();

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
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../spiders-config/tests/fixtures");
    let runtime_config = std::env::temp_dir().join("spiders-cli-bootstrap-trace.json");
    let runtime_dir = fixture_root.join("runtime");
    let authored_config = fixture_root.join("project/config.ts");
    std::fs::write(
        &runtime_config,
        format!(
            r#"{{"layouts":[{{"name":"master-stack","module":"{}","stylesheet":"workspace {{ display: flex; }}"}}]}}"#,
            runtime_dir.join("layouts/master-stack.js").display()
        ),
    )
    .unwrap();

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
    assert_eq!(json["active_seat"], "seat-0");
    assert_eq!(json["active_output"], "bootstrap-output");
    assert_eq!(json["current_workspace"], "bootstrap-workspace");
    assert_eq!(json["focused_window"], "bootstrap-window");
    assert_eq!(json["seat_count"], 1);
    assert_eq!(json["output_count"], 1);
    assert_eq!(json["applied_events"], 0);

    let _ = std::fs::remove_file(runtime_config);
}

#[test]
fn cli_bootstrap_trace_reports_script_failure_in_json_mode() {
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../spiders-config/tests/fixtures");
    let runtime_config = std::env::temp_dir().join("spiders-cli-bootstrap-trace-failure.json");
    let runtime_dir = fixture_root.join("runtime");
    let authored_config = fixture_root.join("project/config.ts");
    let events_path = std::env::temp_dir().join("spiders-cli-bootstrap-events.json");
    std::fs::write(
        &runtime_config,
        format!(
            r#"{{"layouts":[{{"name":"master-stack","module":"{}","stylesheet":"workspace {{ display: flex; }}"}}]}}"#,
            runtime_dir.join("layouts/master-stack.js").display()
        ),
    )
    .unwrap();
    std::fs::write(
        &events_path,
        r#"[{"register-seat":{"seat_name":"seat-x","active":true}},{"remove-output":{"output_id":"missing-output"}}]"#,
    )
    .unwrap();

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
    assert_eq!(json["applied_events"], 1);
    assert_eq!(
        json["failed_event"]["remove-output"]["output_id"],
        "missing-output"
    );
    assert_eq!(json["diagnostics"]["active_seat"], "seat-x");

    let _ = std::fs::remove_file(runtime_config);
    let _ = std::fs::remove_file(events_path);
}
