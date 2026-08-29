// Prevents an extra console window on Windows — Cooldown Bar is macOS-only, but
// keeping the attribute costs nothing and silences the lint in CI images.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cooldown_bar_lib::run();
}
