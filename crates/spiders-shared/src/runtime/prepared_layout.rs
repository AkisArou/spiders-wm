use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedLayout {
    pub name: String,
    pub directory: String,
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedLayout {
    pub selected: SelectedLayout,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub runtime_payload: serde_json::Value,
    #[serde(default)]
    pub stylesheets: PreparedStylesheets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedStylesheet {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PreparedStylesheets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PreparedStylesheet>,
}

impl PreparedStylesheets {
    pub fn combined_source(&self) -> String {
        self.layout
            .as_ref()
            .map(|stylesheet| stylesheet.source.clone())
            .unwrap_or_default()
    }
}
