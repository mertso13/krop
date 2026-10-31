use crate::config::{GroupingMode, SavedFormat};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

const ENGLISH_MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub fn current_local_time() -> SYSTEMTIME {
    unsafe { GetLocalTime() }
}

pub fn period_folder_name(grouping_mode: GroupingMode, capture_time: &SYSTEMTIME) -> String {
    match grouping_mode {
        GroupingMode::Yearly => format!("{:04}", capture_time.wYear),
        GroupingMode::Monthly => format!(
            "{:04}-{:02} {}",
            capture_time.wYear,
            capture_time.wMonth,
            ENGLISH_MONTH_NAMES[usize::from(capture_time.wMonth) - 1]
        ),
        GroupingMode::Daily => format!(
            "{:04}-{:02}-{:02}",
            capture_time.wYear, capture_time.wMonth, capture_time.wDay
        ),
    }
}

pub fn create_unique_capture_file(
    target_directory: &Path,
    capture_time: &SYSTEMTIME,
    saved_format: SavedFormat,
) -> io::Result<(PathBuf, File)> {
    let timestamp_stem = format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        capture_time.wYear,
        capture_time.wMonth,
        capture_time.wDay,
        capture_time.wHour,
        capture_time.wMinute,
        capture_time.wSecond
    );
    let file_extension = saved_format.file_extension();
    let mut collision_number: u32 = 1;
    loop {
        let file_name = if collision_number == 1 {
            format!("{timestamp_stem}.{file_extension}")
        } else {
            format!("{timestamp_stem}_{collision_number}.{file_extension}")
        };
        let candidate_path = target_directory.join(file_name);
        match OpenOptions::new().write(true).create_new(true).open(&candidate_path) {
            Ok(file) => return Ok((candidate_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => collision_number += 1,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{local_time, unique_temporary_directory};

    #[test]
    fn monthly_folders_use_all_twelve_english_month_names_in_sortable_order() {
        let expected_folder_names = [
            "2026-01 January",
            "2026-02 February",
            "2026-03 March",
            "2026-04 April",
            "2026-05 May",
            "2026-06 June",
            "2026-07 July",
            "2026-08 August",
            "2026-09 September",
            "2026-10 October",
            "2026-11 November",
            "2026-12 December",
        ];
        for (month_index, expected_folder_name) in expected_folder_names.iter().enumerate() {
            let capture_time = local_time(2026, month_index as u16 + 1, 15, 0, 0, 0);
            assert_eq!(
                period_folder_name(GroupingMode::Monthly, &capture_time),
                *expected_folder_name
            );
        }
    }

    #[test]
    fn yearly_and_daily_folders_use_flat_zero_padded_names() {
        let capture_time = local_time(2026, 3, 7, 14, 35, 12);
        assert_eq!(period_folder_name(GroupingMode::Yearly, &capture_time), "2026");
        assert_eq!(period_folder_name(GroupingMode::Daily, &capture_time), "2026-03-07");
    }

    #[test]
    fn single_captures_use_the_extension_of_each_saved_format() {
        let target_directory = unique_temporary_directory("save-path-single-formats");
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        for (saved_format, expected_extension) in
            [(SavedFormat::Png, "png"), (SavedFormat::Jpg, "jpg")]
        {
            let (created_path, _) =
                create_unique_capture_file(&target_directory, &capture_time, saved_format).unwrap();

            assert_eq!(
                created_path,
                target_directory.join(format!("2026-10-30_14-35-12.{expected_extension}"))
            );
        }
    }

    #[test]
    fn same_second_captures_get_incrementing_suffixes_without_overwriting() {
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        for (saved_format, extension) in [(SavedFormat::Png, "png"), (SavedFormat::Jpg, "jpg")] {
            let target_directory =
                unique_temporary_directory(&format!("save-path-collisions-{extension}"));

            let created_file_names: Vec<String> = (0..3)
                .map(|_| {
                    let (created_path, _) =
                        create_unique_capture_file(&target_directory, &capture_time, saved_format)
                            .unwrap();
                    created_path.file_name().unwrap().to_string_lossy().into_owned()
                })
                .collect();

            assert_eq!(
                created_file_names,
                [
                    format!("2026-10-30_14-35-12.{extension}"),
                    format!("2026-10-30_14-35-12_2.{extension}"),
                    format!("2026-10-30_14-35-12_3.{extension}")
                ]
            );
        }
    }

    #[test]
    fn missing_target_directory_returns_error_for_each_saved_format() {
        let missing_directory = unique_temporary_directory("save-path-missing").join("absent");
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        for saved_format in [SavedFormat::Png, SavedFormat::Jpg] {
            assert!(
                create_unique_capture_file(&missing_directory, &capture_time, saved_format).is_err()
            );
        }
    }
}
