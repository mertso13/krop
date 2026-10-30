use std::fs;
use std::path::PathBuf;

pub fn unique_temporary_directory(test_name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("krop-test-{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}
