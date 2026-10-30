use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_SNAPSHOT};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_HOTKEY, WM_KEYDOWN, WM_SYSKEYDOWN,
};
use windows::core::Result;

pub const PRTSC_HOTKEY_ID: i32 = 1;

static KEYBOARD_HOOK_TARGET_WINDOW: AtomicIsize = AtomicIsize::new(0);

pub struct PrtScHotkey {
    window: HWND,
    registration: HotkeyRegistration,
}

enum HotkeyRegistration {
    SystemHotkey,
    LowLevelKeyboardHook(HHOOK),
}

pub fn register_prtsc_hotkey(window: HWND) -> Result<PrtScHotkey> {
    if unsafe { RegisterHotKey(Some(window), PRTSC_HOTKEY_ID, MOD_NOREPEAT, VK_SNAPSHOT.0 as u32) }.is_ok() {
        return Ok(PrtScHotkey {
            window,
            registration: HotkeyRegistration::SystemHotkey,
        });
    }
    install_low_level_keyboard_hook(window)
}

fn install_low_level_keyboard_hook(window: HWND) -> Result<PrtScHotkey> {
    KEYBOARD_HOOK_TARGET_WINDOW.store(window.0 as isize, Ordering::SeqCst);
    match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_procedure), None, 0) } {
        Ok(keyboard_hook) => Ok(PrtScHotkey {
            window,
            registration: HotkeyRegistration::LowLevelKeyboardHook(keyboard_hook),
        }),
        Err(hook_error) => {
            KEYBOARD_HOOK_TARGET_WINDOW.store(0, Ordering::SeqCst);
            Err(hook_error)
        }
    }
}

impl Drop for PrtScHotkey {
    fn drop(&mut self) {
        unsafe {
            match &self.registration {
                HotkeyRegistration::SystemHotkey => {
                    let _ = UnregisterHotKey(Some(self.window), PRTSC_HOTKEY_ID);
                }
                HotkeyRegistration::LowLevelKeyboardHook(keyboard_hook) => {
                    KEYBOARD_HOOK_TARGET_WINDOW.store(0, Ordering::SeqCst);
                    let _ = UnhookWindowsHookEx(*keyboard_hook);
                }
            }
        }
    }
}

unsafe extern "system" fn low_level_keyboard_procedure(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let target_window = HWND(KEYBOARD_HOOK_TARGET_WINDOW.load(Ordering::SeqCst) as *mut _);
        handle_low_level_keyboard_event(code, wparam, lparam, target_window)
    }
}

unsafe fn handle_low_level_keyboard_event(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
    target_window: HWND,
) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && !target_window.is_invalid() {
            let keyboard_event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let is_key_down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if keyboard_event.vkCode == VK_SNAPSHOT.0 as u32 && is_key_down {
                let post_result = PostMessageW(
                    Some(target_window),
                    WM_HOTKEY,
                    WPARAM(PRTSC_HOTKEY_ID as usize),
                    LPARAM(0),
                );
                if post_result.is_ok() {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MessageOnlyWindow;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_KEYUP};

    fn keyboard_event(virtual_key: u16) -> KBDLLHOOKSTRUCT {
        KBDLLHOOKSTRUCT {
            vkCode: virtual_key as u32,
            ..Default::default()
        }
    }

    fn event_lparam(keyboard_event: &KBDLLHOOKSTRUCT) -> LPARAM {
        LPARAM(keyboard_event as *const KBDLLHOOKSTRUCT as isize)
    }

    fn take_hotkey_message(window: HWND) -> Option<MSG> {
        let mut message = MSG::default();
        let has_message =
            unsafe { PeekMessageW(&mut message, Some(window), WM_HOTKEY, WM_HOTKEY, PM_REMOVE).as_bool() };
        has_message.then_some(message)
    }

    #[test]
    fn hotkey_registration_can_be_acquired_and_released_repeatedly() {
        let window = MessageOnlyWindow::create();
        {
            let _hotkey = register_prtsc_hotkey(window.handle()).unwrap();
        }
        let _hotkey = register_prtsc_hotkey(window.handle()).unwrap();
    }

    #[test]
    fn low_level_hook_posts_a_hotkey_message_and_swallows_prtsc() {
        let window = MessageOnlyWindow::create();
        let prtsc_press = keyboard_event(VK_SNAPSHOT.0);

        let handled = unsafe {
            handle_low_level_keyboard_event(
                HC_ACTION as i32,
                WPARAM(WM_KEYDOWN as usize),
                event_lparam(&prtsc_press),
                window.handle(),
            )
        };

        assert_eq!(handled.0, 1);
        let hotkey_message = take_hotkey_message(window.handle()).expect("PrtSc must post a hotkey message");
        assert_eq!(hotkey_message.wParam.0, PRTSC_HOTKEY_ID as usize);
    }

    #[test]
    fn low_level_hook_lets_other_keys_and_releases_through() {
        let window = MessageOnlyWindow::create();
        let prtsc_release = keyboard_event(VK_SNAPSHOT.0);
        let other_press = keyboard_event(VK_ESCAPE.0);

        unsafe {
            handle_low_level_keyboard_event(
                HC_ACTION as i32,
                WPARAM(WM_KEYUP as usize),
                event_lparam(&prtsc_release),
                window.handle(),
            );
            handle_low_level_keyboard_event(
                HC_ACTION as i32,
                WPARAM(WM_KEYDOWN as usize),
                event_lparam(&other_press),
                window.handle(),
            );
        }

        assert!(take_hotkey_message(window.handle()).is_none());
    }
}
