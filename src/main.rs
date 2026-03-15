mod cell;
mod grid;
mod terminal;

fn main() -> anyhow::Result<()> {
    use portable_pty::CommandBuilder;
    use portable_pty::native_pty_system;

    println!("PTY作成");
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(portable_pty::PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 1920 / 2,
        pixel_height: 1080,
    })?;

    println!("シェル起動");
    let cmd = CommandBuilder::new("bash");
    let mut child = pair.slave.spawn_command(cmd)?;

    println!("パイプの構築");
    let mut reader = pair.master.try_clone_reader()?; // slave の出力を読む
    let mut writer = pair.master.take_writer()?; // slave に入力を送る

    writer.write_all(b"\x1b[1;1R")?;

    println!("lsコマンドのテスト");
    writer.write_all("ls\n".as_bytes())?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    let mut output = [0; 1 << 12];
    let len = reader.read(&mut output)?;
    print!("{}", String::from_utf8_lossy(&output[..len]));

    println!("exitコマンドのテスト");
    writer.write_all("exit\n".as_bytes())?;
    child.wait()?;

    Ok(())
}
