use std::fs;
use std::path::PathBuf;
use windows::Win32::Foundation::SYSTEMTIME;

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
