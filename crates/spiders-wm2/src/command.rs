use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::WorkspaceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    ReloadConfig,
    DumpTransaction,
    SwitchWorkspace(WorkspaceId),
    RefreshLayoutArtifacts,
    DumpGeometry,
    DumpLayoutTree,
    ListOutputs,
    ListWorkspaces,
    ListWindows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub ok: bool,
    pub message: String,
    pub payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CommandEnvelope {
    command: String,
    #[serde(default)]
    workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct CommandResponse<'a> {
    ok: bool,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<&'a Value>,
}

impl RuntimeCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();

        if trimmed.starts_with('{') {
            let envelope = serde_json::from_str::<CommandEnvelope>(trimmed).ok()?;
            return Self::from_parts(&envelope.command, envelope.workspace_id.as_deref());
        }

        Self::from_parts(trimmed, None)
    }

    fn from_parts(command: &str, workspace_id: Option<&str>) -> Option<Self> {
        match command {
            "reload-config" => Some(Self::ReloadConfig),
            "dump-transaction" => Some(Self::DumpTransaction),
            "refresh-layout-artifacts" => Some(Self::RefreshLayoutArtifacts),
            "dump-geometry" => Some(Self::DumpGeometry),
            "dump-layout-tree" => Some(Self::DumpLayoutTree),
            "list-outputs" => Some(Self::ListOutputs),
            "list-workspaces" => Some(Self::ListWorkspaces),
            "list-windows" => Some(Self::ListWindows),
            "switch-workspace" => {
                workspace_id.map(|id| Self::SwitchWorkspace(WorkspaceId::from(id)))
            }
            _ => None,
        }
    }
}

impl CommandResult {
    pub fn to_json(&self) -> String {
        serde_json::to_string(&CommandResponse {
            ok: self.ok,
            message: &self.message,
            payload: self.payload.as_ref(),
        })
        .unwrap_or_else(|_| format!("{{\"ok\":false,\"message\":\"{}\"}}", self.message))
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandResult, RuntimeCommand};
    use crate::model::WorkspaceId;

    #[test]
    fn parses_reload_config_command() {
        assert_eq!(
            RuntimeCommand::parse("reload-config\n"),
            Some(RuntimeCommand::ReloadConfig)
        );
    }

    #[test]
    fn parses_dump_transaction_command() {
        assert_eq!(
            RuntimeCommand::parse("dump-transaction"),
            Some(RuntimeCommand::DumpTransaction)
        );
    }

    #[test]
    fn parses_dump_geometry_command() {
        assert_eq!(
            RuntimeCommand::parse("dump-geometry"),
            Some(RuntimeCommand::DumpGeometry)
        );
    }

    #[test]
    fn parses_dump_layout_tree_command() {
        assert_eq!(
            RuntimeCommand::parse("dump-layout-tree"),
            Some(RuntimeCommand::DumpLayoutTree)
        );
    }

    #[test]
    fn parses_json_switch_workspace_command() {
        assert_eq!(
            RuntimeCommand::parse("{\"command\":\"switch-workspace\",\"workspace_id\":\"2\"}"),
            Some(RuntimeCommand::SwitchWorkspace(WorkspaceId::from("2")))
        );
    }

    #[test]
    fn parses_json_list_outputs_command() {
        assert_eq!(
            RuntimeCommand::parse("{\"command\":\"list-outputs\"}"),
            Some(RuntimeCommand::ListOutputs)
        );
    }

    #[test]
    fn encodes_command_result_as_json() {
        let json = CommandResult {
            ok: true,
            message: "done".into(),
            payload: None,
        }
        .to_json();

        assert_eq!(json, "{\"ok\":true,\"message\":\"done\"}");
    }

    #[test]
    fn encodes_command_result_payload_as_json() {
        let json = CommandResult {
            ok: true,
            message: "listed".into(),
            payload: Some(serde_json::json!({"items": [1, 2]})),
        }
        .to_json();

        assert_eq!(
            json,
            "{\"ok\":true,\"message\":\"listed\",\"payload\":{\"items\":[1,2]}}"
        );
    }
}
