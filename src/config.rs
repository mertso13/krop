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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SavedFormat {
    #[default]
    Png,
    Jpg,
}

impl SavedFormat {
    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub base_directory: PathBuf,
    pub grouping_mode: GroupingMode,
    #[serde(default)]
    pub saved_format: SavedFormat,
    #[serde(
        default = "default_jpeg_quality",
        deserialize_with = "deserialize_clamped_jpeg_quality"
    )]
    pub jpeg_quality: u8,
}

impl Config {
    pub fn with_default_settings(pictures_directory: &Path) -> Self {
        Self {
            base_directory: pictures_directory.join("Screenshots"),
            grouping_mode: GroupingMode::default(),
            saved_format: SavedFormat::default(),
            jpeg_quality: default_jpeg_quality(),
        }
    }

    pub fn load_or_initialize() -> io::Result<Self> {
        let default_config = Self::with_default_settings(&known_folder_path(&FOLDERID_Pictures)?);
        Self::load_or_initialize_at(&config_file_path()?, default_config)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&config_file_path()?)
    }

    pub fn refresh_jpeg_quality_from_disk(&mut self) {
        if let Ok(config_path) = config_file_path() {
            self.refresh_jpeg_quality_from(&config_path);
        }
    }

    fn refresh_jpeg_quality_from(&mut self, config_path: &Path) {
        let Ok(config_text) = fs::read_to_string(config_path) else {
            return;
        };
        let Ok(disk_config) = toml::from_str::<Self>(&config_text) else {
            return;
        };
        self.jpeg_quality = disk_config.jpeg_quality;
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

fn default_jpeg_quality() -> u8 {
    90
}

fn deserialize_clamped_jpeg_quality<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(i64::deserialize(deserializer)?.clamp(1, 100) as u8)
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
        assert_eq!(config.saved_format, SavedFormat::Png);
        assert_eq!(config.jpeg_quality, 90);
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
                saved_format: SavedFormat::Jpg,
                jpeg_quality: 42,
            };
            saved_config.save_to(&config_path).unwrap();

            let loaded_config =
                Config::load_or_initialize_at(&config_path, Config::with_default_settings(&test_directory))
                    .unwrap();

            assert_eq!(loaded_config, saved_config);
        }
    }

    #[test]
    fn legacy_config_without_format_fields_keeps_existing_settings_and_defaults_new_ones() {
        let test_directory = unique_temporary_directory("config-legacy-upgrade");
        let config_path = test_directory.join("config.toml");
        fs::write(
            &config_path,
            "base_directory = 'D:\\Captures'\ngrouping_mode = 'Daily'\n",
        )
        .unwrap();

        let loaded_config = Config::load_or_initialize_at(
            &config_path,
            Config::with_default_settings(&test_directory),
        )
        .unwrap();

        assert_eq!(loaded_config.base_directory, PathBuf::from(r"D:\Captures"));
        assert_eq!(loaded_config.grouping_mode, GroupingMode::Daily);
        assert_eq!(loaded_config.saved_format, SavedFormat::Png);
        assert_eq!(loaded_config.jpeg_quality, 90);
    }

    #[test]
    fn out_of_range_jpeg_quality_values_are_clamped_without_resetting_other_settings() {
        let test_directory = unique_temporary_directory("config-quality-clamping");
        let config_path = test_directory.join("config.toml");
        for (requested_quality, expected_quality) in
            [(-20, 1), (0, 1), (1, 1), (90, 90), (100, 100), (150, 100), (300, 100)]
        {
            fs::write(
                &config_path,
                format!(
                    "base_directory = 'D:\\Captures'\ngrouping_mode = 'Yearly'\nsaved_format = 'Jpg'\njpeg_quality = {requested_quality}\n"
                ),
            )
            .unwrap();

            let loaded_config = Config::load_or_initialize_at(
                &config_path,
                Config::with_default_settings(&test_directory),
            )
            .unwrap();

            assert_eq!(
                loaded_config.jpeg_quality, expected_quality,
                "unexpected clamped quality for {requested_quality}"
            );
            assert_eq!(loaded_config.base_directory, PathBuf::from(r"D:\Captures"));
            assert_eq!(loaded_config.grouping_mode, GroupingMode::Yearly);
            assert_eq!(loaded_config.saved_format, SavedFormat::Jpg);
        }
    }

    #[test]
    fn jpeg_quality_refreshes_from_the_config_file_on_disk() {
        let test_directory = unique_temporary_directory("config-quality-refresh");
        let config_path = test_directory.join("config.toml");
        let mut config = Config::with_default_settings(&test_directory);
        config.save_to(&config_path).unwrap();
        let edited_config_text = fs::read_to_string(&config_path)
            .unwrap()
            .replace("jpeg_quality = 90", "jpeg_quality = 35");
        fs::write(&config_path, edited_config_text).unwrap();

        config.refresh_jpeg_quality_from(&config_path);

        assert_eq!(config.jpeg_quality, 35);
    }

    #[test]
    fn jpeg_quality_refresh_keeps_the_current_value_when_the_file_is_missing_or_malformed() {
        let test_directory = unique_temporary_directory("config-quality-refresh-fallback");
        let config_path = test_directory.join("config.toml");
        let mut config = Config::with_default_settings(&test_directory);
        config.jpeg_quality = 55;

        config.refresh_jpeg_quality_from(&config_path);
        assert_eq!(config.jpeg_quality, 55);

        fs::write(&config_path, "not = [valid").unwrap();
        config.refresh_jpeg_quality_from(&config_path);
        assert_eq!(config.jpeg_quality, 55);
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
