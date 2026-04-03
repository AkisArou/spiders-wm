use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "requires a live graphical session and intentionally runs the real wm binary"]
fn wm_live_ipc_smoke_uses_test_config() {
    if std::env::var_os("SPIDERS_WM_RUN_LIVE_SMOKE").is_none() {
        eprintln!("skipping live wm smoke test; set SPIDERS_WM_RUN_LIVE_SMOKE=1 to enable");
        return;
    }

    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skipping live wm smoke test; no DISPLAY or WAYLAND_DISPLAY is available");
        return;
    }

    let workspace_root = workspace_root();
    let test_config_root = workspace_root.join("test_config");
    let authored_config = test_config_root.join("config.ts");
    assert!(
        authored_config.exists(),
        "missing test_config authored config"
    );

    let temp_root = unique_temp_root("wm-live-ipc-smoke");
    std::fs::create_dir_all(&temp_root).unwrap();
    let cache_dir = temp_root.join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let ipc_socket = temp_root.join("wm.sock");

    let mut wm = spawn_wm(&workspace_root, &authored_config, &cache_dir, &ipc_socket);

    wait_for_socket(&ipc_socket, Duration::from_secs(20));

    let query_output = run_cli(
        &workspace_root,
        &ipc_socket,
        ["ipc-query", "--json", "--query", "workspace-names"],
    );
    assert!(
        query_output.status.success(),
        "ipc-query failed: {}",
        String::from_utf8_lossy(&query_output.stderr)
    );

    let query_json: serde_json::Value = serde_json::from_slice(&query_output.stdout).unwrap();
    assert_eq!(query_json["status"], "ok");
    assert_eq!(query_json["response"]["type"], "workspace-names");
    assert_eq!(query_json["response"]["payload"][0], "1");

    let command_output = run_cli(
        &workspace_root,
        &ipc_socket,
        ["ipc-command", "--json", "--command", "reload-config"],
    );
    assert!(
        command_output.status.success(),
        "ipc-command failed: {}",
        String::from_utf8_lossy(&command_output.stderr)
    );

    let command_json: serde_json::Value = serde_json::from_slice(&command_output.stdout).unwrap();
    assert_eq!(command_json["status"], "ok");
    assert_eq!(command_json["response_kind"], "command-accepted");

    terminate_child(&mut wm);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn unique_temp_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{label}-{nanos}"))
}

fn spawn_wm(
    workspace_root: &Path,
    authored_config: &Path,
    cache_dir: &Path,
    ipc_socket: &Path,
) -> Child {
    Command::new(env!("CARGO_BIN_EXE_spiders-wm"))
        .current_dir(workspace_root)
        .env("SPIDERS_WM_AUTHORED_CONFIG", authored_config)
        .env("SPIDERS_WM_CACHE_DIR", cache_dir)
        .env("SPIDERS_WM_IPC_SOCKET", ipc_socket)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn wm live smoke process")
}

fn run_cli<const N: usize>(
    workspace_root: &Path,
    ipc_socket: &Path,
    args: [&str; N],
) -> std::process::Output {
    Command::new("cargo")
        .current_dir(workspace_root)
        .args(["run", "-q", "--manifest-path"])
        .arg(workspace_root.join("Cargo.toml"))
        .args(["-p", "spiders-cli", "--"])
        .args(args)
        .env("SPIDERS_WM_IPC_SOCKET", ipc_socket)
        .output()
        .expect("failed to run spiders-cli during live smoke test")
}

fn wait_for_socket(socket_path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if socket_path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }

    panic!(
        "timed out waiting for wm IPC socket at {}",
        socket_path.display()
    );
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
