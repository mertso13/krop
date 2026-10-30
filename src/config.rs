use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Pictures, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GroupingMode {
    Daily,
    #[default]
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub base_directory: PathBuf,
    pub grouping_mode: GroupingMode,
}

impl Config {
    pub fn with_default_settings(pictures_directory: &Path) -> Self {
        Self {
            base_directory: pictures_directory.join("Screenshots"),
            grouping_mode: GroupingMode::default(),
        }
    }

    pub fn load_or_initialize() -> io::Result<Self> {
        let default_config = Self::with_default_settings(&known_folder_path(&FOLDERID_Pictures)?);
        Self::load_or_initialize_at(&config_file_path()?, default_config)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&config_file_path()?)
    }

    fn load_or_initialize_at(config_path: &Path, default_config: Self) -> io::Result<Self> {
        match fs::read_to_string(config_path) {
            Ok(config_text) => Ok(toml::from_str(&config_text).unwrap_or(default_config)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                default_config.save_to(config_path)?;
                Ok(default_config)
            }
            Err(error) => Err(error),
        }
    }

    fn save_to(&self, config_path: &Path) -> io::Result<()> {
        if let Some(config_directory) = config_path.parent() {
            fs::create_dir_all(config_directory)?;
        }
        let config_text = toml::to_string(self).map_err(io::Error::other)?;
        let temporary_path = config_path.with_extension("toml.tmp");
        fs::write(&temporary_path, config_text)?;
        fs::rename(&temporary_path, config_path)
    }
}

fn config_file_path() -> io::Result<PathBuf> {
    Ok(known_folder_path(&FOLDERID_RoamingAppData)?
        .join("krop")
        .join("config.toml"))
}

fn known_folder_path(folder_id: &GUID) -> io::Result<PathBuf> {
    unsafe {
        let folder_path_pointer = SHGetKnownFolderPath(folder_id, KF_FLAG_DEFAULT, None)?;
        let folder_path = OsString::from_wide(folder_path_pointer.as_wide());
        CoTaskMemFree(Some(folder_path_pointer.0 as *const _));
        Ok(PathBuf::from(folder_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temporary_directory;

    #[test]
    fn default_settings_use_screenshots_folder_and_monthly_mode() {
        let pictures_directory = PathBuf::from(r"C:\Users\Example\Pictures");
        let config = Config::with_default_settings(&pictures_directory);
        assert_eq!(
            config.base_directory,
            PathBuf::from(r"C:\Users\Example\Pictures\Screenshots")
        );
        assert_eq!(config.grouping_mode, GroupingMode::Monthly);
    }

    #[test]
    fn first_load_creates_config_file_with_defaults() {
        let test_directory = unique_temporary_directory("config-first-load");
        let config_path = test_directory.join("krop").join("config.toml");
        let default_config = Config::with_default_settings(&test_directory);

        let loaded_config =
            Config::load_or_initialize_at(&config_path, default_config.clone()).unwrap();

        assert_eq!(loaded_config, default_config);
        assert!(config_path.exists());
        let persisted_config: Config =
            toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(persisted_config, default_config);
    }

    #[test]
    fn saved_config_round_trips_through_toml() {
        let test_directory = unique_temporary_directory("config-round-trip");
        let config_path = test_directory.join("config.toml");
        for grouping_mode in [GroupingMode::Daily, GroupingMode::Monthly, GroupingMode::Yearly] {
            let saved_config = Config {
                base_directory: PathBuf::from(r"D:\Captures\Ünicode Folder"),
                grouping_mode,
            };
            saved_config.save_to(&config_path).unwrap();

            let loaded_config =
                Config::load_or_initialize_at(&config_path, Config::with_default_settings(&test_directory))
                    .unwrap();

            assert_eq!(loaded_config, saved_config);
        }
    }

    #[test]
    fn corrupt_config_falls_back_to_defaults_without_overwriting_file() {
        let test_directory = unique_temporary_directory("config-corrupt");
        let config_path = test_directory.join("config.toml");
        fs::write(&config_path, "not = [valid").unwrap();
        let default_config = Config::with_default_settings(&test_directory);

        let loaded_config =
            Config::load_or_initialize_at(&config_path, default_config.clone()).unwrap();

        assert_eq!(loaded_config, default_config);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "not = [valid");
    }

    #[test]
    fn known_pictures_folder_resolves_to_existing_directory() {
        assert!(known_folder_path(&FOLDERID_Pictures).unwrap().is_dir());
    }
}
