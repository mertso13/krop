use crate::bgra_image::{BYTES_PER_PIXEL, BgraImage};
use crate::config::GroupingMode;
use crate::save_path::{create_unique_png_file, period_folder_name};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use windows::Win32::Foundation::{HWND, LPARAM, SYSTEMTIME, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_APP_SAVE_RESULT: u32 = WM_APP + 1;
pub const SAVE_SUCCEEDED: usize = 1;
pub const SAVE_FAILED: usize = 0;

pub fn spawn_png_save_worker(
    image: BgraImage,
    base_directory: PathBuf,
    grouping_mode: GroupingMode,
    capture_time: SYSTEMTIME,
    result_window: HWND,
) {
    let result_window_address = result_window.0 as usize;
    thread::spawn(move || {
        let save_status = match save_png_capture(&image, &base_directory, grouping_mode, &capture_time) {
            Ok(_) => SAVE_SUCCEEDED,
            Err(_) => SAVE_FAILED,
        };
        unsafe {
            let _ = PostMessageW(
                Some(HWND(result_window_address as *mut _)),
                WM_APP_SAVE_RESULT,
                WPARAM(save_status),
                LPARAM(0),
            );
        }
    });
}

pub fn save_png_capture(
    image: &BgraImage,
    base_directory: &Path,
    grouping_mode: GroupingMode,
    capture_time: &SYSTEMTIME,
) -> io::Result<PathBuf> {
    let target_directory = base_directory.join(period_folder_name(grouping_mode, capture_time));
    fs::create_dir_all(&target_directory)?;
    let (png_path, png_file) = create_unique_png_file(&target_directory, capture_time)?;
    if let Err(write_error) = write_png(image, png_file) {
        let _ = fs::remove_file(&png_path);
        return Err(write_error);
    }
    Ok(png_path)
}

fn write_png(image: &BgraImage, png_file: File) -> io::Result<()> {
    let mut buffered_file = BufWriter::new(png_file);
    let mut encoder = png::Encoder::new(&mut buffered_file, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header().map_err(io::Error::other)?;
    let rgb_pixels: Vec<u8> = image
        .top_down_pixels()
        .as_chunks::<BYTES_PER_PIXEL>()
        .0
        .iter()
        .flat_map(|bgra_pixel| [bgra_pixel[2], bgra_pixel[1], bgra_pixel[0]])
        .collect();
    png_writer.write_image_data(&rgb_pixels).map_err(io::Error::other)?;
    png_writer.finish().map_err(io::Error::other)?;
    buffered_file.flush()?;
    buffered_file.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MessageOnlyWindow, local_time, unique_temporary_directory};
    use std::io::BufReader;
    use std::time::{Duration, Instant};
    use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW};

    fn three_by_two_test_image() -> BgraImage {
        BgraImage::new(
            3,
            2,
            vec![
                10, 20, 30, 0, 40, 50, 60, 0, 70, 80, 90, 0,
                100, 110, 120, 0, 130, 140, 150, 0, 160, 170, 180, 0,
            ],
        )
    }

    fn decode_rgb_png(png_path: &Path) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(BufReader::new(File::open(png_path).unwrap()));
        let mut png_reader = decoder.read_info().unwrap();
        let mut decoded_pixels = vec![0; png_reader.output_buffer_size().unwrap()];
        let frame_info = png_reader.next_frame(&mut decoded_pixels).unwrap();
        assert_eq!(frame_info.color_type, png::ColorType::Rgb);
        decoded_pixels.truncate(frame_info.buffer_size());
        (frame_info.width, frame_info.height, decoded_pixels)
    }

    #[test]
    fn saved_png_is_lossless_and_placed_in_period_folder() {
        let base_directory = unique_temporary_directory("save-worker-lossless");
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        let png_path = save_png_capture(
            &three_by_two_test_image(),
            &base_directory,
            GroupingMode::Monthly,
            &capture_time,
        )
        .unwrap();

        assert_eq!(
            png_path,
            base_directory.join("2026-10 October").join("2026-10-30_14-35-12.png")
        );
        let (width, height, rgb_pixels) = decode_rgb_png(&png_path);
        assert_eq!((width, height), (3, 2));
        assert_eq!(
            rgb_pixels,
            [30, 20, 10, 60, 50, 40, 90, 80, 70, 120, 110, 100, 150, 140, 130, 180, 170, 160]
        );
    }

    #[test]
    fn unwritable_base_directory_returns_error() {
        let test_directory = unique_temporary_directory("save-worker-unwritable");
        let file_used_as_base_directory = test_directory.join("not-a-directory");
        fs::write(&file_used_as_base_directory, b"").unwrap();

        let save_result = save_png_capture(
            &three_by_two_test_image(),
            &file_used_as_base_directory,
            GroupingMode::Daily,
            &local_time(2026, 10, 30, 14, 35, 12),
        );

        assert!(save_result.is_err());
    }

    #[test]
    fn worker_posts_save_result_to_window() {
        let result_window = MessageOnlyWindow::create();
        let base_directory = unique_temporary_directory("save-worker-post");

        spawn_png_save_worker(
            three_by_two_test_image(),
            base_directory.clone(),
            GroupingMode::Yearly,
            local_time(2026, 10, 30, 14, 35, 12),
            result_window.handle(),
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut message = MSG::default();
        let received_message = loop {
            let has_message = unsafe {
                PeekMessageW(
                    &mut message,
                    Some(result_window.handle()),
                    WM_APP_SAVE_RESULT,
                    WM_APP_SAVE_RESULT,
                    PM_REMOVE,
                )
            };
            if has_message.as_bool() {
                break true;
            }
            if Instant::now() > deadline {
                break false;
            }
            thread::sleep(Duration::from_millis(5));
        };

        assert!(received_message);
        assert_eq!(message.wParam.0, SAVE_SUCCEEDED);
        assert!(base_directory.join("2026").join("2026-10-30_14-35-12.png").is_file());
    }
}
