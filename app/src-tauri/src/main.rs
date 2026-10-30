//! Prism desktop entry point. The app itself lives in `lib.rs` so Android can load it as a library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    prism_app_lib::run()
}
