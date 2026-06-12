mod app;
mod cli;

fn main() -> anyhow::Result<()> {
    cli::run(std::env::args().skip(1).collect())
}
