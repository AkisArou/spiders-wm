mod app;
mod backend;
mod cli;
mod config;

fn main() -> anyhow::Result<()> {
    app::run()
}
