pub mod ast;
pub mod css;
pub mod matching;
pub mod pipeline;
pub mod stylo_adapter;

pub fn crate_ready() -> bool {
    true
}
