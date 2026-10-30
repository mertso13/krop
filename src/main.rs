#![cfg_attr(not(test), windows_subsystem = "windows")]

mod autostart;
mod bgra_image;
mod clipboard;
mod config;
mod save_path;
#[cfg(test)]
mod test_support;

fn main() {}
