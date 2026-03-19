use std::collections::HashMap;

use crate::model::{OutputId, WindowId};

#[derive(Debug, Default)]
pub struct TopologyState {
    pub windows: HashMap<WindowId, WindowNode>,
    pub outputs: HashMap<OutputId, crate::model::OutputNode>,
}

#[derive(Debug, Clone)]
pub struct WindowNode {
    pub alive: bool,
    pub mapped: bool,
    pub title: Option<String>,
    pub app_id: Option<String>,
}
