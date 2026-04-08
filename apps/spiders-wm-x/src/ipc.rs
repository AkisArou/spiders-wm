use std::path::PathBuf;

use anyhow::Result;
use spiders_ipc::{DebugDumpKind, DebugResponse};

pub(crate) fn handle_debug_dump(
    kind: DebugDumpKind,
    state_json: &str,
) -> Result<DebugResponse, String> {
    match kind {
        DebugDumpKind::WmState => {
            let path =
                dump_text("wm-state.json", state_json)?.map(|path| path.display().to_string());
            Ok(DebugResponse::DumpWritten { kind, path })
        }
        unsupported => Err(format!("wm-x does not support debug dump `{unsupported:?}` yet")),
    }
}

fn dump_text(file_name: &str, contents: &str) -> Result<Option<PathBuf>, String> {
    let Some(output_dir) = configured_debug_output_dir() else {
        return Ok(None);
    };

    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let path = output_dir.join(file_name);
    std::fs::write(&path, contents).map_err(|error| error.to_string())?;
    Ok(Some(path))
}

fn configured_debug_output_dir() -> Option<PathBuf> {
    std::env::var_os("SPIDERS_WM_DEBUG_OUTPUT_DIR").map(PathBuf::from).or_else(|| {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .or_else(|| Some(std::env::temp_dir()))
            .map(|base| base.join(format!("spiders-wm-x-debug-{}", std::process::id())))
    })
}
