use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostQuitMessage,
    RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW, WM_DESTROY,
};
use windows::core::{Error, PCWSTR, Result, w};

const APPLICATION_WINDOW_CLASS: PCWSTR = w!("KropApplicationWindow");
const APPLICATION_WINDOW_TITLE: PCWSTR = w!("krop");

pub struct ApplicationWindow(HWND);

impl ApplicationWindow {
    pub fn create() -> Result<Self> {
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
        Ok(Self(window))
    }

    pub fn handle(&self) -> HWND {
        self.0
    }
}

impl Drop for ApplicationWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
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
        if message == WM_DESTROY {
            PostQuitMessage(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, WM_QUIT};

    #[test]
    fn application_window_is_hidden_and_quits_the_message_loop_when_destroyed() {
        let application_window = ApplicationWindow::create().unwrap();
        assert!(!unsafe { IsWindowVisible(application_window.handle()) }.as_bool());

        drop(application_window);

        let mut message = MSG::default();
        assert_eq!(unsafe { GetMessageW(&mut message, None, 0, 0) }.0, 0);
        assert_eq!(message.message, WM_QUIT);
    }
}
