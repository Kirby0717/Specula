mod core;
mod gui;

fn main() -> anyhow::Result<()> {
    log_init();

    gui::run_app()?;

    Ok(())
}

fn log_init() {
    use simplelog::{
        CombinedLogger, Config, LevelFilter, SimpleLogger, WriteLogger,
    };

    CombinedLogger::init(vec![
        SimpleLogger::new(LevelFilter::Info, Config::default()),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            std::fs::File::create("specula.log").unwrap(),
        ),
    ])
    .unwrap();
}
