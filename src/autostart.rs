use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, Result, w};

const RUN_KEY_PATH: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const AUTOSTART_VALUE_NAME: &str = "krop";

pub fn is_autostart_enabled() -> bool {
    run_value_exists(AUTOSTART_VALUE_NAME)
}

pub fn enable_autostart() -> Result<()> {
    let executable_path = std::env::current_exe()?;
    set_run_value(AUTOSTART_VALUE_NAME, &autostart_command(&executable_path))
}

pub fn disable_autostart() -> Result<()> {
    delete_run_value(AUTOSTART_VALUE_NAME)
}

fn autostart_command(executable_path: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(executable_path);
    command.push("\"");
    command
}

fn run_value_exists(value_name: &str) -> bool {
    let value_name = HSTRING::from(value_name);
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY_PATH,
            &value_name,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

fn set_run_value(value_name: &str, command: &OsStr) -> Result<()> {
    let value_name = HSTRING::from(value_name);
    let nul_terminated_command: Vec<u16> = command.encode_wide().chain(Some(0)).collect();
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY_PATH,
            &value_name,
            REG_SZ.0,
            Some(nul_terminated_command.as_ptr().cast()),
            (nul_terminated_command.len() * size_of::<u16>()) as u32,
        )
        .ok()
    }
}

fn delete_run_value(value_name: &str) -> Result<()> {
    let value_name = HSTRING::from(value_name);
    let delete_result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY_PATH, &value_name) };
    if delete_result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    delete_result.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn autostart_command_quotes_executable_path_with_spaces() {
        let executable_path = PathBuf::from(r"C:\Program Files\krop\krop.exe");
        assert_eq!(
            autostart_command(&executable_path),
            OsString::from(r#""C:\Program Files\krop\krop.exe""#)
        );
    }

    #[test]
    fn run_key_path_points_to_current_user_run_key() {
        assert_eq!(
            unsafe { RUN_KEY_PATH.to_string() }.unwrap(),
            r"Software\Microsoft\Windows\CurrentVersion\Run"
        );
    }

    #[test]
    fn run_value_can_be_enabled_queried_and_disabled() {
        let test_value_name = format!("krop-test-{}", std::process::id());
        let command = autostart_command(Path::new(r"C:\krop test\krop.exe"));

        set_run_value(&test_value_name, &command).unwrap();
        assert!(run_value_exists(&test_value_name));

        delete_run_value(&test_value_name).unwrap();
        assert!(!run_value_exists(&test_value_name));

        delete_run_value(&test_value_name).unwrap();
    }
}
