// Copyright 2025 Bloxide, all rights reserved
//! Format generated Rust source through `rustfmt`.

/// Format a Rust source string through `rustfmt`.
///
/// Falls back to the unformatted input if `rustfmt` is not available,
/// so the codegen still works in environments without rustfmt installed.
pub fn rustfmt_source(source: &str) -> anyhow::Result<String> {
    use std::io::Write;
    use std::process::Command;

    let spawn_result = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(_) => {
            // rustfmt not available — return the source as-is.
            return Ok(source.to_string());
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if output.status.success() {
        let formatted = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(formatted)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "bloxide: warning: rustfmt failed, returning unformatted code:\n{}",
            stderr
        );
        Ok(source.to_string())
    }
}
