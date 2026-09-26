// Hide the console window in ALL builds (debug included).
// Logs go to <desktop_logs_dir>/grokfree.log instead of stdout — see init_logging().
#![windows_subsystem = "windows"]

fn main() {
    grokfree_lib::run();
}
