mod app;
mod backend;
mod cli;
mod config;
mod ipc;

fn main() -> anyhow::Result<()> {
    app::run()
}
