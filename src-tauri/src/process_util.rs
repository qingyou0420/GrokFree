//! Windows helpers so background CLI tools don't flash a console window.
//!
//! Sidebar project/session switches call `git status` (and similar helpers).
//! Without `CREATE_NO_WINDOW`, each spawn shows a brief black console on Windows.

use std::ffi::OsStr;
use std::process::{Command, Stdio};

/// Build a `Command` that will not show a console window on Windows.
pub fn silent_command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    hide_console(&mut cmd);
    // Avoid attaching/allocating a console for I/O even when flags are ignored
    // by a particular host (defensive; output() still captures stdout/stderr).
    cmd.stdin(Stdio::null());
    cmd
}

/// `Command::output()` with a hard deadline: poll `try_wait`, kill on timeout.
///
/// Probe children (`grok --version`, `where grok`) have hung in the wild
/// (AV scans, network drives on PATH, CLI update checks). An `.output()` call
/// with no deadline leaks one process per attempt and wedges the caller —
/// during a session-spawn storm that is exactly the "many grok processes in
/// Task Manager" symptom. Output is expected to be small (< pipe buffer);
/// a child that fills the pipe and stalls is treated as timed out and killed.
pub fn output_with_timeout(
    mut cmd: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Apply `CREATE_NO_WINDOW` on Windows (no-op elsewhere).
///
/// Also set stdout/stderr to piped by default when callers later call `output()`;
/// stdin is nulled in `silent_command` so git/where never attach a console.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000 — console apps run without a window.
        // Combine with CREATE_NEW_PROCESS_GROUP so Ctrl+C in the UI doesn't
        // cascade, without using DETACHED_PROCESS (which breaks output capture).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    let _ = cmd;
}
