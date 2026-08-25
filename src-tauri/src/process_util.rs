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
