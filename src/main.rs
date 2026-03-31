#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod gui;

fn main() -> anyhow::Result<()> {
    let config = config::Config::load().unwrap_or_default();

    log_init(config.log_level);
    setup_panic_hook();

    gui::run_app(config)?;

    Ok(())
}

fn log_init(log_level: log::LevelFilter) {
    use simplelog::{
        CombinedLogger, Config, SharedLogger, SimpleLogger, WriteLogger,
    };

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![];
    loggers.push(WriteLogger::new(
        log_level,
        Config::default(),
        std::fs::File::create("specula.log").unwrap(),
    ));
    if cfg!(debug_assertions) {
        loggers.push(SimpleLogger::new(log_level, Config::default()));
    }
    CombinedLogger::init(loggers).unwrap();
}
fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("パニック: {info}");
    }));
}
