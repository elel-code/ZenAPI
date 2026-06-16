slint::include_modules!();

mod cli;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return cli::run(args);
    }
    let app = App::new()?;
    app.run()?;
    Ok(())
}
