#![cfg_attr(not(test), windows_subsystem = "windows")]

mod autostart;
mod config;
mod save_path;
#[cfg(test)]
mod test_support;

fn main() {}
