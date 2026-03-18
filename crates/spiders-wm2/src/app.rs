use crate::{
    bindings::SmithayBindings,
    state::{TopologyState, WmState},
};

#[derive(Debug, Default)]
pub struct AppState {
    pub topology: TopologyState,
    pub wm: WmState,
    pub bindings: SmithayBindings,
}
