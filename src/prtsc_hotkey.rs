use windows::Win32::Foundation::HWND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_SNAPSHOT,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, MessageBoxW, SW_SHOWNORMAL,
};
use windows::core::{PCWSTR, Result, w};

pub const PRTSC_HOTKEY_ID: i32 = 1;
pub const PRTSC_SNIPPING_POLL_TIMER_ID: usize = 1;
pub const PRTSC_SNIPPING_POLL_INTERVAL_MILLISECONDS: u32 = 300;

const CURRENT_VERSION_KEY_PATH: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
const KEYBOARD_SETTINGS_KEY_PATH: PCWSTR = w!(r"Control Panel\Keyboard");
const CURRENT_BUILD_NUMBER_VALUE_NAME: PCWSTR = w!("CurrentBuildNumber");
const PRTSC_SNIPPING_VALUE_NAME: PCWSTR = w!("PrintScreenKeyForSnippingEnabled");
const FIRST_WINDOWS_11_BUILD_NUMBER: u32 = 22000;
const FIRST_DEVICES_KEYBOARD_BUILD_NUMBER: u32 = 26100;
const DEVICES_KEYBOARD_SETTINGS_URI: PCWSTR = w!("ms-settings:devices-keyboard");
const ACCESSIBILITY_KEYBOARD_SETTINGS_URI: PCWSTR = w!("ms-settings:easeofaccess-keyboard");

pub struct PrtScHotkey {
    window: HWND,
}

pub fn register_prtsc_hotkey(window: HWND) -> Result<PrtScHotkey> {
    unsafe {
        RegisterHotKey(
            Some(window),
            PRTSC_HOTKEY_ID,
            MOD_NOREPEAT,
            VK_SNAPSHOT.0 as u32,
        )?
    };
    Ok(PrtScHotkey { window })
}

impl Drop for PrtScHotkey {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.window), PRTSC_HOTKEY_ID);
        }
    }
}

#[derive(Default)]
pub struct PrtScSnippingSettleState {
    saw_os_prtsc_snipping_disabled_once: bool,
}

impl PrtScSnippingSettleState {
    pub fn should_register_prtsc_hotkey(&mut self, os_prtsc_snipping_active: bool) -> bool {
        if os_prtsc_snipping_active {
            self.saw_os_prtsc_snipping_disabled_once = false;
            return false;
        }
        if self.saw_os_prtsc_snipping_disabled_once {
            return true;
        }
        self.saw_os_prtsc_snipping_disabled_once = true;
        false
    }
}

pub fn is_os_prtsc_snipping_active() -> bool {
    let Some(build_number) = read_current_build_number() else {
        return false;
    };
    is_os_prtsc_snipping_enabled(build_number, read_prtsc_snipping_registry_value())
}

pub fn show_prtsc_hotkey_conflict_message() {
    unsafe {
        let _ = MessageBoxW(
            None,
            w!(
                "PrtSc is already in use by another application.\n\nClose the application that is using PrtSc and start krop again."
            ),
            w!("krop"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn prompt_to_disable_os_prtsc_snipping() {
    let user_choice = unsafe {
        MessageBoxW(
            None,
            w!(
                "Windows is using the Print Screen key to open screen capture, so krop cannot capture with PrtSc.\n\nOpen Windows Keyboard Settings to turn off \"Use the Print screen key to open screen capture\"?"
            ),
            w!("krop"),
            MB_OKCANCEL | MB_ICONINFORMATION,
        )
    };
    if user_choice == IDOK {
        open_os_prtsc_keyboard_settings();
    }
}

fn open_os_prtsc_keyboard_settings() {
    let settings_uri = read_current_build_number().map_or(
        DEVICES_KEYBOARD_SETTINGS_URI,
        keyboard_settings_uri_for_build,
    );
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), settings_uri, None, None, SW_SHOWNORMAL);
    }
}

fn keyboard_settings_uri_for_build(build_number: u32) -> PCWSTR {
    if build_number >= FIRST_DEVICES_KEYBOARD_BUILD_NUMBER {
        DEVICES_KEYBOARD_SETTINGS_URI
    } else {
        ACCESSIBILITY_KEYBOARD_SETTINGS_URI
    }
}

fn is_os_prtsc_snipping_enabled(build_number: u32, registry_value: Option<u32>) -> bool {
    if build_number >= FIRST_WINDOWS_11_BUILD_NUMBER {
        registry_value != Some(0)
    } else {
        matches!(registry_value, Some(value) if value != 0)
    }
}

fn read_current_build_number() -> Option<u32> {
    let mut build_number_buffer = [0u16; 32];
    let mut build_number_buffer_bytes = (build_number_buffer.len() * size_of::<u16>()) as u32;
    let read_result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            CURRENT_VERSION_KEY_PATH,
            CURRENT_BUILD_NUMBER_VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(build_number_buffer.as_mut_ptr().cast()),
            Some(&mut build_number_buffer_bytes),
        )
    };
    if read_result.is_err() {
        return None;
    }
    let string_end = build_number_buffer
        .iter()
        .position(|&character| character == 0)
        .unwrap_or(build_number_buffer.len());
    String::from_utf16(&build_number_buffer[..string_end])
        .ok()?
        .parse()
        .ok()
}

fn read_prtsc_snipping_registry_value() -> Option<u32> {
    let mut registry_value = 0u32;
    let mut registry_value_bytes = size_of::<u32>() as u32;
    let read_result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEYBOARD_SETTINGS_KEY_PATH,
            PRTSC_SNIPPING_VALUE_NAME,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut registry_value as *mut u32).cast()),
            Some(&mut registry_value_bytes),
        )
    };
    read_result.is_ok().then_some(registry_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MessageOnlyWindow;

    #[test]
    fn hotkey_registration_excludes_a_second_window_until_released() {
        let first_window = MessageOnlyWindow::create();
        let second_window = MessageOnlyWindow::create();

        let first_hotkey = register_prtsc_hotkey(first_window.handle()).unwrap();
        assert!(register_prtsc_hotkey(second_window.handle()).is_err());

        drop(first_hotkey);
        let _second_hotkey = register_prtsc_hotkey(second_window.handle()).unwrap();
    }

    #[test]
    fn windows_11_treats_an_absent_or_non_zero_snipping_setting_as_active() {
        assert!(is_os_prtsc_snipping_enabled(22000, None));
        assert!(is_os_prtsc_snipping_enabled(26100, Some(1)));
        assert!(!is_os_prtsc_snipping_enabled(26100, Some(0)));
    }

    #[test]
    fn windows_10_treats_only_a_non_zero_snipping_setting_as_active() {
        assert!(!is_os_prtsc_snipping_enabled(21999, None));
        assert!(is_os_prtsc_snipping_enabled(19045, Some(1)));
        assert!(!is_os_prtsc_snipping_enabled(19045, Some(0)));
    }

    #[test]
    fn snipping_settle_state_registers_only_after_two_consecutive_inactive_ticks() {
        let mut settle_state = PrtScSnippingSettleState::default();

        assert!(!settle_state.should_register_prtsc_hotkey(true));
        assert!(!settle_state.should_register_prtsc_hotkey(false));
        assert!(settle_state.should_register_prtsc_hotkey(false));
    }

    #[test]
    fn snipping_settle_state_restarts_when_os_snipping_becomes_active_again() {
        let mut settle_state = PrtScSnippingSettleState::default();

        assert!(!settle_state.should_register_prtsc_hotkey(false));
        assert!(!settle_state.should_register_prtsc_hotkey(true));
        assert!(!settle_state.should_register_prtsc_hotkey(false));
        assert!(settle_state.should_register_prtsc_hotkey(false));
    }

    #[test]
    fn keyboard_settings_uri_targets_devices_keyboard_on_windows_11_24h2_and_newer() {
        assert_eq!(
            unsafe { keyboard_settings_uri_for_build(26100).to_string() }.unwrap(),
            "ms-settings:devices-keyboard"
        );
        assert_eq!(
            unsafe { keyboard_settings_uri_for_build(22631).to_string() }.unwrap(),
            "ms-settings:easeofaccess-keyboard"
        );
        assert_eq!(
            unsafe { keyboard_settings_uri_for_build(19045).to_string() }.unwrap(),
            "ms-settings:easeofaccess-keyboard"
        );
    }
}
