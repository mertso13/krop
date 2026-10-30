#![cfg_attr(not(test), windows_subsystem = "windows")]

mod autostart;
mod bgra_image;
mod clipboard;
mod config;
mod osd;
mod overlay;
mod save_path;
mod save_worker;
mod screen_capture;
mod selection;
#[cfg(test)]
mod test_support;

fn main() {}
