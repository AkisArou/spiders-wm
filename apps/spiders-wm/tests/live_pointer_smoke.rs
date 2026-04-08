use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "requires a live X11 session and intentionally runs the real wm binary"]
fn wm_live_pointer_smoke_moves_nested_winit_window_pointer() {
    if std::env::var_os("SPIDERS_WM_RUN_LIVE_POINTER_SMOKE").is_none() {
        eprintln!(
            "skipping live wm pointer smoke test; set SPIDERS_WM_RUN_LIVE_POINTER_SMOKE=1 to enable"
        );
        return;
    }

    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("skipping live wm pointer smoke test; no X11 DISPLAY is available for xdotool");
        return;
    }

    let workspace_root = workspace_root();
    let temp_root = unique_temp_root("wm-live-pointer-smoke");
    std::fs::create_dir_all(&temp_root).unwrap();

    let mut wm = spawn_wm(&workspace_root);
    let window_id = wait_for_winit_window(Duration::from_secs(20));

    focus_window(&window_id);
    move_pointer(&window_id, 80, 80);
    move_pointer(&window_id, 140, 120);

    terminate_child(&mut wm);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

fn unique_temp_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("{label}-{nanos}"))
}

fn spawn_wm(workspace_root: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_spiders-wm"))
        .current_dir(workspace_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn wm live pointer smoke process")
}

fn wait_for_winit_window(timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let output = Command::new("xdotool")
            .args(["search", "--name", "Smithay"])
            .output()
            .expect("failed to search for nested winit window with xdotool");
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(window_id) = stdout.lines().next() {
                return window_id.to_string();
            }
        }

        thread::sleep(Duration::from_millis(100));
    }

    panic!("timed out waiting for nested winit window");
}

fn focus_window(window_id: &str) {
    let status = Command::new("xdotool")
        .args(["windowactivate", &window_id])
        .status()
        .expect("failed to focus nested winit window");
    assert!(status.success(), "xdotool windowactivate failed");
}

fn move_pointer(window_id: &str, x: i32, y: i32) {
    let status = Command::new("xdotool")
        .args(["mousemove", "--window", &window_id, &x.to_string(), &y.to_string()])
        .status()
        .expect("failed to move pointer inside nested winit window");
    assert!(status.success(), "xdotool mousemove failed");
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
