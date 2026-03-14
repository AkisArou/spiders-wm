pub mod authored;
pub mod compile;
pub mod graph;
pub mod loader;
pub mod model;
pub mod runtime;
pub mod service;

pub fn crate_ready() -> bool {
    true
}
