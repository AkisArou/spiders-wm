pub mod compile;
pub mod graph;
mod layout_value;
pub mod loader;
mod module_graph;
mod payload;
mod preview_bundle;

pub use layout_value::decode_js_layout_value;
pub use module_graph::{JavaScriptModule, JavaScriptModuleGraph};
pub use payload::{decode_runtime_graph_payload, encode_runtime_graph_payload};
pub use preview_bundle::compile_source_bundle_to_module_graph;

pub fn parse_workspace_names(source: &str) -> Vec<String> {
    let workspaces_source = source
        .split("workspaces:")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_default();
    let mut workspaces = Vec::new();
    let mut remaining = workspaces_source;

    while let Some(start) = remaining.find('"') {
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('"') else {
            break;
        };
        let name = &after_start[..end];
        if !name.is_empty() {
            workspaces.push(name.to_string());
        }
        remaining = &after_start[end + 1..];
    }

    if workspaces.is_empty() {
        vec!["1".to_string(), "2".to_string(), "3".to_string()]
    } else {
        workspaces
    }
}
