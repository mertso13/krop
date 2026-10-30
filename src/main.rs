#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app_window;
mod autostart;
mod bgra_image;
mod clipboard;
mod config;
mod dpi_awareness;
mod osd;
mod overlay;
mod prtsc_hotkey;
mod save_path;
mod save_worker;
mod screen_capture;
mod selection;
mod single_instance;
#[cfg(test)]
mod test_support;

use windows::core::Result;

fn main() {
    if run_application().is_err() {
        std::process::exit(1);
    }
}

fn run_application() -> Result<()> {
    let Some(_single_instance_guard) = single_instance::acquire_single_instance()? else {
        return Ok(());
    };
    let _ = dpi_awareness::enable_per_monitor_v2_awareness();
    let application_window = app_window::ApplicationWindow::create()?;
    let _prtsc_hotkey = prtsc_hotkey::register_prtsc_hotkey(application_window.handle())?;
    app_window::run_message_loop();
    Ok(())
}
