mod core;
mod gui;

fn main() -> anyhow::Result<()> {
    log_init();

    gui::run_app()?;

    return Ok(());

    /*let (mut terminal, handle) =
        terminal::Terminal::new(10, 30, 1_000_000, "bash")?;

    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        terminal.process_pty_output();
    }

    println!("send echo hello");
    terminal.write(b"echo hello\r");

    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        terminal.process_pty_output();
    }

    println!("~~~~~~~~~~");
    println!("{}", terminal.core.dump_visible());

    terminal.write(b"exit\r");
    std::thread::sleep(std::time::Duration::from_millis(100));
    terminal.process_pty_output();

    terminal.pty.wait()?;
    handle.join().ok();
    Ok(())*/
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
