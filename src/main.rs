#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app_window;
mod autostart;
mod bgra_image;
mod clipboard;
mod com_initialization;
mod config;
mod dpi_awareness;
mod folder_picker;
mod osd;
mod overlay;
mod prtsc_hotkey;
mod save_path;
mod save_worker;
mod screen_capture;
mod selection;
mod single_instance;
mod system_tray;
#[cfg(test)]
mod test_support;

use std::error::Error;

fn main() {
    if run_application().is_err() {
        std::process::exit(1);
    }
}

fn run_application() -> Result<(), Box<dyn Error>> {
    let Some(_single_instance_guard) = single_instance::acquire_single_instance()? else {
        return Ok(());
    };
    let _ = dpi_awareness::enable_per_monitor_v2_awareness();
    let _com_initialization = com_initialization::initialize_apartment_threaded()?;
    let configuration = config::Config::load_or_initialize()?;
    let mut application_window = app_window::ApplicationWindow::create(configuration)?;
    let _system_tray_icon = system_tray::add_tray_icon(application_window.handle())?;
    if prtsc_hotkey::is_os_prtsc_snipping_active() {
        application_window.defer_prtsc_hotkey_registration()?;
        prtsc_hotkey::prompt_to_disable_os_prtsc_snipping();
    } else if let Err(registration_error) = application_window.register_prtsc_hotkey() {
        prtsc_hotkey::show_prtsc_hotkey_conflict_message();
        return Err(registration_error.into());
    }
    app_window::run_message_loop();
    Ok(())
}
