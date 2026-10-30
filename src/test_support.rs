use std::fs;
use std::path::PathBuf;
use windows::Win32::Foundation::{HWND, SYSTEMTIME};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows::core::w;

pub fn unique_temporary_directory(test_name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("krop-test-{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

pub fn local_time(year: u16, month: u16, day: u16, hour: u16, minute: u16, second: u16) -> SYSTEMTIME {
    SYSTEMTIME {
        wYear: year,
        wMonth: month,
        wDay: day,
        wHour: hour,
        wMinute: minute,
        wSecond: second,
        ..Default::default()
    }
}

pub struct MessageOnlyWindow(HWND);

impl MessageOnlyWindow {
    pub fn create() -> Self {
        let window_handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!(""),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .unwrap();
        Self(window_handle)
    }

    pub fn handle(&self) -> HWND {
        self.0
    }
}

impl Drop for MessageOnlyWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
