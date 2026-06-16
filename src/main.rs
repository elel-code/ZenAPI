mod app;
mod cli;
mod ui {
    slint::include_modules!();
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    cli::run(args)
}
