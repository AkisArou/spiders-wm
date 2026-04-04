pub mod command;

use crate::state::SpidersWm;
use spiders_core::effect::WmHostEffect;
use spiders_wm_runtime::WmHost;

pub use command::WmCommand;
pub use spiders_wm_runtime::WmRuntime;

pub struct NoopHost;

impl WmHost for NoopHost {
    fn on_effect(&mut self, _effect: WmHostEffect) {}
}

impl SpidersWm {
    pub fn runtime(&mut self) -> WmRuntime<'_> {
        WmRuntime::new(&mut self.model)
    }
}
