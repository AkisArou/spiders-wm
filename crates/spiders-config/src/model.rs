use serde::{Deserialize, Serialize};

use spiders_shared::api::WmAction;
use spiders_shared::wm::LayoutRef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputConfig {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub trigger: String,
    pub action: WmAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub options: ConfigOptions,
    #[serde(default)]
    pub inputs: Vec<InputConfig>,
    #[serde(default)]
    pub outputs: Vec<OutputConfig>,
    #[serde(default)]
    pub layouts: Vec<LayoutRef>,
    #[serde(default)]
    pub rules: Vec<WindowRule>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub autostart: Vec<String>,
    #[serde(default)]
    pub autostart_once: Vec<String>,
}
