pub mod command;

use crate::state::SpidersWm;
pub use command::WmCommand;
pub use spiders_wm_runtime::{RuntimeCommand, RuntimeResult, WmRuntime};

impl SpidersWm {
    pub fn runtime(&mut self) -> WmRuntime<'_> {
        WmRuntime::new(&mut self.model)
    }
}
