use crate::autostart;
use crate::clipboard::copy_image_to_clipboard;
use crate::config::{Config, GroupingMode};
use crate::folder_picker::pick_folder;
use crate::osd::{self, OsdFeedback};
use crate::overlay;
use crate::save_path::current_local_time;
use crate::save_worker::{SAVE_SUCCEEDED, WM_APP_SAVE_RESULT, spawn_png_save_worker};
use crate::screen_capture::capture_virtual_screen;
use crate::system_tray::{self, TrayCommand};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassW, SW_SHOWNORMAL, SetWindowLongPtrW,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY,
    WM_LBUTTONDBLCLK, WM_NCDESTROY, WM_RBUTTONUP, WNDCLASSW,
};
use windows::core::{Error, PCWSTR, Result, w};

const APPLICATION_WINDOW_CLASS: PCWSTR = w!("KropApplicationWindow");
const APPLICATION_WINDOW_TITLE: PCWSTR = w!("krop");

pub struct ApplicationWindow {
    window: HWND,
    _state: Box<ApplicationState>,
}

impl ApplicationWindow {
    pub fn create(config: Config) -> Result<Self> {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None)?.0 });
        register_application_window_class(instance)?;
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                APPLICATION_WINDOW_CLASS,
                APPLICATION_WINDOW_TITLE,
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        let mut state = Box::new(ApplicationState {
            config,
            capture_in_progress: false,
        });
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut *state as *mut ApplicationState) as isize,
            );
        }
        Ok(Self {
            window,
            _state: state,
        })
    }

    pub fn handle(&self) -> HWND {
        self.window
    }
}

impl Drop for ApplicationWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}

struct ApplicationState {
    config: Config,
    capture_in_progress: bool,
}

pub fn run_message_loop() {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn register_application_window_class(instance: HINSTANCE) -> Result<()> {
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(application_window_procedure),
        hInstance: instance,
        lpszClassName: APPLICATION_WINDOW_CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        let registration_error = unsafe { GetLastError() };
        if registration_error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

unsafe extern "system" fn application_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCDESTROY {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
        }
        if message == WM_DESTROY {
            PostQuitMessage(0);
        }

        let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut ApplicationState;
        if state_pointer.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }

        match message {
            WM_HOTKEY => handle_prtsc_hotkey(window, state_pointer),
            WM_APP_SAVE_RESULT => show_save_feedback(wparam),
            system_tray::WM_APP_SYSTEM_TRAY => handle_tray_callback(window, state_pointer, lparam),
            _ => {}
        }

        DefWindowProcW(window, message, wparam, lparam)
    }
}

unsafe fn handle_prtsc_hotkey(window: HWND, state_pointer: *mut ApplicationState) {
    unsafe {
        if (*state_pointer).capture_in_progress {
            return;
        }
        (*state_pointer).capture_in_progress = true;
        let base_directory = (*state_pointer).config.base_directory.clone();
        let grouping_mode = (*state_pointer).config.grouping_mode;
        run_capture_session(window, base_directory, grouping_mode);
        (*state_pointer).capture_in_progress = false;
    }
}

fn run_capture_session(window: HWND, base_directory: PathBuf, grouping_mode: GroupingMode) {
    let Ok(frozen_screen) = capture_virtual_screen() else {
        return;
    };
    let Ok(Some(cropped_image)) = overlay::select_region(frozen_screen) else {
        return;
    };
    let _ = copy_image_to_clipboard(window, &cropped_image);
    spawn_png_save_worker(
        cropped_image,
        base_directory,
        grouping_mode,
        current_local_time(),
        window,
    );
}

fn show_save_feedback(save_result: WPARAM) {
    let feedback = if save_result.0 == SAVE_SUCCEEDED {
        OsdFeedback::CaptureSaved
    } else {
        OsdFeedback::SaveFailed
    };
    let _ = osd::show_feedback(feedback);
}

unsafe fn handle_tray_callback(window: HWND, state_pointer: *mut ApplicationState, lparam: LPARAM) {
    unsafe {
        let tray_notification = (lparam.0 as u32) & 0xFFFF;
        if tray_notification == WM_LBUTTONDBLCLK {
            open_screenshot_folder(&(*state_pointer).config.base_directory);
        } else if tray_notification == WM_RBUTTONUP || tray_notification == WM_CONTEXTMENU {
            let config = (*state_pointer).config.clone();
            if let Ok(Some(command)) = system_tray::show_context_menu(window, &config) {
                apply_tray_command(window, state_pointer, command);
            }
        }
    }
}

unsafe fn apply_tray_command(window: HWND, state_pointer: *mut ApplicationState, command: TrayCommand) {
    unsafe {
        match command {
            TrayCommand::OpenFolder => open_screenshot_folder(&(*state_pointer).config.base_directory),
            TrayCommand::ChangeSaveFolder => {
                if let Ok(Some(selected_directory)) = pick_folder(window) {
                    (*state_pointer).config.base_directory = selected_directory;
                    let _ = (*state_pointer).config.save();
                }
            }
            TrayCommand::SetGroupingMode(grouping_mode) => {
                (*state_pointer).config.grouping_mode = grouping_mode;
                let _ = (*state_pointer).config.save();
            }
            TrayCommand::ToggleAutostart => toggle_autostart(),
            TrayCommand::Exit => {
                let _ = DestroyWindow(window);
            }
        }
    }
}

fn toggle_autostart() {
    let toggle_result = if autostart::is_autostart_enabled() {
        autostart::disable_autostart()
    } else {
        autostart::enable_autostart()
    };
    let _ = toggle_result;
}

fn open_screenshot_folder(base_directory: &Path) {
    let _ = std::fs::create_dir_all(base_directory);
    let mut folder_path: Vec<u16> = base_directory.as_os_str().encode_wide().collect();
    folder_path.push(0);
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(folder_path.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, WM_QUIT};

    fn test_config() -> Config {
        Config {
            base_directory: std::env::temp_dir().join("krop-test-application-window"),
            grouping_mode: GroupingMode::Monthly,
        }
    }

    #[test]
    fn application_window_is_hidden_and_quits_the_message_loop_when_destroyed() {
        let application_window = ApplicationWindow::create(test_config()).unwrap();
        assert!(!unsafe { IsWindowVisible(application_window.handle()) }.as_bool());

        drop(application_window);

        let mut message = MSG::default();
        assert_eq!(unsafe { GetMessageW(&mut message, None, 0, 0) }.0, 0);
        assert_eq!(message.message, WM_QUIT);
    }

    #[test]
    fn application_window_stores_its_configuration() {
        let application_window = ApplicationWindow::create(test_config()).unwrap();
        assert_eq!(application_window._state.config, test_config());
    }
}
