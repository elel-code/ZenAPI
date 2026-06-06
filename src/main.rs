mod app;
mod ui {
    slint::include_modules!();
}

fn main() -> anyhow::Result<()> {
    app::run()
}
