use std::process::Command;
use std::{io::Write, os::unix::net::UnixListener};
use tempfile::TempDir;

use spiders_core::event::WmEvent;
use spiders_core::query::QueryResponse;
use spiders_ipc::{
    DebugDumpKind, DebugResponse, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic,
    encode_response_line,
};

fn cli_bin() -> String {
    env!("CARGO_BIN_EXE_spiders-cli").to_string()
}

fn runtime_fixture_paths() -> (std::path::PathBuf, std::path::PathBuf) {
    let fixture_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../config/tests/fixtures");
    (fixture_root.join("runtime"), fixture_root.join("project/config.ts"))
}

fn write_prepared_config() -> (TempDir, std::path::PathBuf) {
    let (runtime_dir, _) = runtime_fixture_paths();
    let runtime_root = TempDir::new().unwrap();
    std::fs::create_dir_all(runtime_root.path().join("layouts/master-stack")).unwrap();
    let layout_source =
        std::fs::read_to_string(runtime_dir.join("layouts/master-stack.js")).unwrap();
    std::fs::write(
        runtime_root.path().join("layouts/master-stack/index.js"),
        format!("export default ({});", layout_source.trim()),
    )
    .unwrap();
    std::fs::write(
        runtime_root.path().join("config.js"),
        r#"
            export default {
              layouts: { default: "master-stack" }
            };
        "#,
    )
    .unwrap();
    let prepared_config = runtime_root.path().join("config.js");
    (runtime_root, prepared_config)
}

#[test]
fn cli_reports_discovery_in_json_mode() {
    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("config")
        .arg("discover")
        .env("SPIDERS_WM_AUTHORED_CONFIG", "/tmp/authored.js")
        .env("SPIDERS_WM_CACHE_DIR", "/tmp/spiders-cache")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["authored_config"], "/tmp/authored.js");
    assert_eq!(json["prepared_config"], "/tmp/spiders-cache/config.js");
}

#[test]
fn cli_check_config_reports_validation_errors_in_json_mode() {
    let runtime_root = TempDir::new().unwrap();
    let prepared_config = runtime_root.path().join("config.js");
    std::fs::write(
        &prepared_config,
        r#"
            export default {
              layouts: { default: "missing" }
            };
        "#,
    )
    .unwrap();

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("config")
        .arg("check")
        .env("SPIDERS_WM_AUTHORED_CONFIG", "/tmp/authored.js")
        .env("SPIDERS_WM_CACHE_DIR", prepared_config.parent().unwrap())
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["phase"], "load");
    assert!(json["message"].as_str().unwrap().contains("missing"));
}

#[test]
fn cli_check_config_reports_success_in_json_mode_with_fixture_layout() {
    let (_, authored_config) = runtime_fixture_paths();
    let (_runtime_root, prepared_config) = write_prepared_config();

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("config")
        .arg("check")
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_CACHE_DIR", prepared_config.parent().unwrap())
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["layouts"], 1);
}

#[test]
fn cli_build_config_writes_prepared_config_with_module_graphs() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("layouts/master-stack")).unwrap();
    std::fs::write(
        root.path().join("config.ts"),
        r#"
            export default {
              layouts: { default: "master-stack" },
            };
        "#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("layouts/master-stack/index.ts"),
        r#"
            export default function layout() {
              return { type: "workspace", children: [] };
            }
        "#,
    )
    .unwrap();
    std::fs::write(root.path().join("layouts/master-stack/index.css"), "workspace {}").unwrap();

    let authored_config = root.path().join("config.ts");
    let runtime_root = TempDir::new().unwrap();
    let prepared_config = runtime_root.path().join("config.js");

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("config")
        .arg("build")
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_CACHE_DIR", prepared_config.parent().unwrap())
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["layouts"], 1);

    let built = std::fs::read_to_string(&prepared_config).unwrap();
    assert!(built.contains("export default"));
    assert!(std::fs::metadata(runtime_root.path().join("layouts/master-stack/index.js")).is_ok());
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
                QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]),
            )))
            .unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("query")
        .arg("workspace-names")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["query"], "workspace-names");
    assert_eq!(json["response"]["type"], "workspace-names");
    assert_eq!(json["response"]["payload"][0], "1");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_command_reports_socket_response_in_json_mode() {
    let socket_path = unique_socket_path("cli-ipc-command");
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
                encode_response_line(&IpcEnvelope::new(IpcServerMessage::CommandAccepted)).unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("command")
        .arg("close-focused-window")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["command"]["type"], "close-focused-window");
    assert_eq!(json["response_kind"], "command-accepted");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_debug_reports_dump_response_in_json_mode() {
    let socket_path = unique_socket_path("cli-ipc-debug");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            reader.read_line(&mut request).unwrap();
            let line = encode_response_line(&IpcEnvelope::new(IpcServerMessage::Debug(
                DebugResponse::DumpWritten {
                    kind: DebugDumpKind::SceneSnapshot,
                    path: Some("/tmp/scene-snapshot.json".into()),
                },
            )))
            .unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("debug")
        .arg("dump")
        .arg("scene-snapshot")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["dump_kind"], "scene-snapshot");
    assert_eq!(json["path"], "/tmp/scene-snapshot.json");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_debug_supports_frame_sync_dump_kind() {
    let socket_path = unique_socket_path("cli-ipc-debug-frame-sync");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            reader.read_line(&mut request).unwrap();
            let line = encode_response_line(&IpcEnvelope::new(IpcServerMessage::Debug(
                DebugResponse::DumpWritten {
                    kind: DebugDumpKind::FrameSync,
                    path: Some("/tmp/frame-sync.json".into()),
                },
            )))
            .unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("debug")
        .arg("dump")
        .arg("frame-sync")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["dump_kind"], "frame-sync");
    assert_eq!(json["path"], "/tmp/frame-sync.json");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_query_uses_default_socket_env_when_flag_is_omitted() {
    let socket_path = unique_socket_path("cli-ipc-query-env");
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
                QueryResponse::WorkspaceNames(vec!["1".into()]),
            )))
            .unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("query")
        .arg("workspace-names")
        .env("SPIDERS_WM_IPC_SOCKET", &socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["socket_path"], socket_path.display().to_string());
    assert_eq!(json["response"]["type"], "workspace-names");
    assert_eq!(json["response"]["payload"][0], "1");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_ipc_monitor_reports_streamed_events_in_json_mode() {
    let socket_path = unique_socket_path("cli-ipc-monitor");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn({
        let socket_path = socket_path.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            reader.read_line(&mut request).unwrap();
            let subscribed =
                encode_response_line(&IpcEnvelope::new(IpcServerMessage::Subscribed {
                    topics: vec![IpcSubscriptionTopic::Layout],
                }))
                .unwrap();
            let event = encode_response_line(&IpcEnvelope::new(IpcServerMessage::event(
                WmEvent::LayoutChange { workspace_id: None, layout: None },
            )))
            .unwrap();
            stream.write_all(subscribed.as_bytes()).unwrap();
            stream.write_all(event.as_bytes()).unwrap();
            socket_path
        }
    });

    let output = Command::new(cli_bin())
        .arg("--json")
        .arg("wm")
        .arg("monitor")
        .arg("layout")
        .arg("--socket")
        .arg(&socket_path)
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["topics"][0], "layout");
    assert_eq!(json["subscribed_topics"][0], "layout");
    assert_eq!(json["events"][0]["type"], "layout-change");

    let path = handle.join().unwrap();
    let _ = std::fs::remove_file(path);
}

fn unique_socket_path(label: &str) -> std::path::PathBuf {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("spiders-cli-test-{label}-{nanos}.sock"))
}
