use anyhow::{Context, Result, ensure};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub fn run(command: &mut Command) -> Result<Vec<u8>> {
    let output = command
        .output()
        .with_context(|| format!("Starting {command:?}"))?;
    ensure!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

pub fn text(command: &mut Command) -> Result<String> {
    Ok(String::from_utf8(run(command)?)?)
}

pub fn input(command: &mut Command, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().context("Child stdin")?;
    // Feed and drain concurrently: archive extractors can produce output before
    // consuming the complete (potentially large) input archive.
    let output = std::thread::scope(|scope| -> Result<_> {
        let writer = scope.spawn(move || stdin.write_all(bytes));
        let output = child.wait_with_output()?;
        writer
            .join()
            .map_err(|_| anyhow::anyhow!("Input writer panicked"))??;
        Ok(output)
    })?;
    ensure!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}
