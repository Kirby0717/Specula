#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod gui;

fn main() -> anyhow::Result<()> {
    log_init();
    setup_panic_hook();

    let config = config::Config::load().unwrap_or_default();

    gui::run_app(config)?;

    Ok(())
}

fn log_init() {
    use simplelog::{
        CombinedLogger, Config, LevelFilter, SharedLogger, SimpleLogger,
        WriteLogger,
    };

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![];
    loggers.push(WriteLogger::new(
        LevelFilter::Info,
        Config::default(),
        std::fs::File::create("specula.log").unwrap(),
    ));
    if cfg!(debug_assertions) {
        loggers.push(SimpleLogger::new(LevelFilter::Info, Config::default()));
    }
    CombinedLogger::init(loggers).unwrap();
}
fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("パニック: {info}");
    }));
}
