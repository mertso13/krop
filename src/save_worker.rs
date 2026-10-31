use crate::bgra_image::{BYTES_PER_PIXEL, BgraImage};
use crate::com_initialization;
use crate::config::{GroupingMode, SavedFormat};
use crate::save_path::{create_unique_capture_file, period_folder_name};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use windows::Win32::Foundation::{E_POINTER, HGLOBAL, HWND, LPARAM, SYSTEMTIME, WPARAM};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR,
    IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, IPropertyBag2, PROPBAG2,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{Error, PWSTR};

pub const WM_APP_SAVE_RESULT: u32 = WM_APP + 1;
pub const SAVE_SUCCEEDED: usize = 1;
pub const SAVE_FAILED: usize = 0;

pub fn spawn_save_worker(
    image: BgraImage,
    base_directory: PathBuf,
    grouping_mode: GroupingMode,
    saved_format: SavedFormat,
    jpeg_quality: u8,
    capture_time: SYSTEMTIME,
    result_window: HWND,
) {
    let result_window_address = result_window.0 as usize;
    thread::spawn(move || {
        let save_status = match save_capture(
            &image,
            &base_directory,
            grouping_mode,
            saved_format,
            jpeg_quality,
            &capture_time,
        ) {
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

pub fn save_capture(
    image: &BgraImage,
    base_directory: &Path,
    grouping_mode: GroupingMode,
    saved_format: SavedFormat,
    jpeg_quality: u8,
    capture_time: &SYSTEMTIME,
) -> io::Result<PathBuf> {
    let target_directory = base_directory.join(period_folder_name(grouping_mode, capture_time));
    fs::create_dir_all(&target_directory)?;
    let (capture_path, capture_file) =
        create_unique_capture_file(&target_directory, capture_time, saved_format)?;
    let write_result = match saved_format {
        SavedFormat::Png => write_png(image, capture_file),
        SavedFormat::Jpg => write_jpeg(image, capture_file, jpeg_quality),
    };
    if let Err(write_error) = write_result {
        let _ = fs::remove_file(&capture_path);
        return Err(write_error);
    }
    Ok(capture_path)
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

fn write_jpeg(image: &BgraImage, jpeg_file: File, jpeg_quality: u8) -> io::Result<()> {
    let _com_initialization =
        com_initialization::initialize_apartment_threaded().map_err(io::Error::other)?;
    let jpeg_bytes = encode_jpeg(image, jpeg_quality).map_err(io::Error::other)?;
    let mut buffered_file = BufWriter::new(jpeg_file);
    buffered_file.write_all(&jpeg_bytes)?;
    buffered_file.flush()?;
    buffered_file.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()
}

fn encode_jpeg(image: &BgraImage, jpeg_quality: u8) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let imaging_factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let output_stream = CreateStreamOnHGlobal(HGLOBAL(std::ptr::null_mut()), true)?;
        let jpeg_encoder =
            imaging_factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        jpeg_encoder.Initialize(&output_stream, WICBitmapEncoderNoCache)?;

        let mut frame_encoder: Option<IWICBitmapFrameEncode> = None;
        let mut encoder_options: Option<IPropertyBag2> = None;
        jpeg_encoder.CreateNewFrame(&mut frame_encoder, &mut encoder_options)?;
        let frame_encoder = frame_encoder.ok_or_else(|| Error::from(E_POINTER))?;
        let encoder_options = encoder_options.ok_or_else(|| Error::from(E_POINTER))?;
        let quality_variant = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_R4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        fltVal: f32::from(jpeg_quality) / 100.0,
                    },
                }),
            },
        };
        let mut quality_property_name: Vec<u16> = "ImageQuality\0".encode_utf16().collect();
        let quality_property = PROPBAG2 {
            pstrName: PWSTR(quality_property_name.as_mut_ptr()),
            ..Default::default()
        };
        encoder_options.Write(1, &quality_property, &quality_variant)?;

        frame_encoder.Initialize(&encoder_options)?;
        frame_encoder.SetSize(image.width(), image.height())?;
        let mut pixel_format = GUID_WICPixelFormat24bppBGR;
        frame_encoder.SetPixelFormat(&mut pixel_format)?;
        let bgr_pixels: Vec<u8> = image
            .top_down_pixels()
            .as_chunks::<BYTES_PER_PIXEL>()
            .0
            .iter()
            .flat_map(|bgra_pixel| [bgra_pixel[0], bgra_pixel[1], bgra_pixel[2]])
            .collect();
        frame_encoder.WritePixels(image.height(), image.width() * 3, &bgr_pixels)?;
        frame_encoder.Commit()?;
        jpeg_encoder.Commit()?;

        output_stream.Seek(0, STREAM_SEEK_SET, None)?;
        let mut stream_statistics = STATSTG::default();
        output_stream.Stat(&mut stream_statistics, STATFLAG_NONAME)?;
        let mut jpeg_bytes = vec![0u8; stream_statistics.cbSize as usize];
        let mut bytes_read = 0u32;
        output_stream
            .Read(
                jpeg_bytes.as_mut_ptr() as *mut std::ffi::c_void,
                jpeg_bytes.len() as u32,
                Some(&mut bytes_read),
            )
            .ok()?;
        jpeg_bytes.truncate(bytes_read as usize);
        Ok(jpeg_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MessageOnlyWindow, local_time, unique_temporary_directory};
    use std::io::BufReader;
    use std::time::{Duration, Instant};
    use windows::Win32::Graphics::Imaging::WICDecodeMetadataCacheOnDemand;
    use windows::Win32::UI::Shell::SHCreateMemStream;
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

    fn photographic_test_image() -> BgraImage {
        let width = 128u32;
        let height = 128u32;
        let mut pixels = Vec::with_capacity(width as usize * height as usize * BYTES_PER_PIXEL);
        let mut random_state = 0x9E37_79B9u32;
        for y in 0..height {
            for x in 0..width {
                random_state = random_state
                    .wrapping_mul(1_664_525)
                    .wrapping_add(1_013_904_223);
                let noise = (random_state >> 24) as i32 - 128;
                let red = (x as i32 * 2 + noise).clamp(1, 255) as u8;
                let green = (y as i32 * 2 + noise / 2).clamp(1, 255) as u8;
                let blue = ((x + y) as i32 + noise / 3).clamp(1, 255) as u8;
                pixels.extend_from_slice(&[blue, green, red, 0]);
            }
        }
        BgraImage::new(width, height, pixels)
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

    fn decode_jpeg_dimensions(jpeg_path: &Path) -> (u32, u32) {
        let _com_initialization =
            com_initialization::initialize_apartment_threaded().expect("failed to initialize COM");
        let jpeg_bytes = fs::read(jpeg_path).unwrap();
        unsafe {
            let imaging_factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let stream = SHCreateMemStream(Some(&jpeg_bytes)).expect("failed to open JPEG stream");
            let decoder = imaging_factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .unwrap();
            let frame = decoder.GetFrame(0).unwrap();
            let mut width = 0u32;
            let mut height = 0u32;
            frame.GetSize(&mut width, &mut height).unwrap();
            (width, height)
        }
    }

    #[test]
    fn saved_png_is_lossless_and_placed_in_period_folder() {
        let base_directory = unique_temporary_directory("save-worker-lossless");
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        let png_path = save_capture(
            &three_by_two_test_image(),
            &base_directory,
            GroupingMode::Monthly,
            SavedFormat::Png,
            90,
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
    fn saved_jpeg_has_captured_dimensions_and_extension() {
        let base_directory = unique_temporary_directory("save-worker-jpeg-dimensions");
        let capture_time = local_time(2026, 10, 30, 14, 35, 12);

        let jpeg_path = save_capture(
            &three_by_two_test_image(),
            &base_directory,
            GroupingMode::Monthly,
            SavedFormat::Jpg,
            90,
            &capture_time,
        )
        .unwrap();

        assert_eq!(
            jpeg_path,
            base_directory.join("2026-10 October").join("2026-10-30_14-35-12.jpg")
        );
        assert_eq!(decode_jpeg_dimensions(&jpeg_path), (3, 2));
    }

    #[test]
    fn lower_jpeg_quality_produces_smaller_files_with_matching_dimensions() {
        let base_directory = unique_temporary_directory("save-worker-jpeg-quality");
        let image = photographic_test_image();

        let low_quality_path = save_capture(
            &image,
            &base_directory,
            GroupingMode::Daily,
            SavedFormat::Jpg,
            30,
            &local_time(2026, 10, 30, 14, 35, 12),
        )
        .unwrap();
        let high_quality_path = save_capture(
            &image,
            &base_directory,
            GroupingMode::Daily,
            SavedFormat::Jpg,
            95,
            &local_time(2026, 10, 30, 14, 35, 13),
        )
        .unwrap();

        assert_eq!(decode_jpeg_dimensions(&low_quality_path), (128, 128));
        assert_eq!(decode_jpeg_dimensions(&high_quality_path), (128, 128));
        let low_quality_size = fs::metadata(&low_quality_path).unwrap().len();
        let high_quality_size = fs::metadata(&high_quality_path).unwrap().len();
        assert!(low_quality_size < high_quality_size);
    }

    #[test]
    fn unwritable_base_directory_returns_error() {
        let test_directory = unique_temporary_directory("save-worker-unwritable");
        let file_used_as_base_directory = test_directory.join("not-a-directory");
        fs::write(&file_used_as_base_directory, b"").unwrap();

        for saved_format in [SavedFormat::Png, SavedFormat::Jpg] {
            let save_result = save_capture(
                &three_by_two_test_image(),
                &file_used_as_base_directory,
                GroupingMode::Daily,
                saved_format,
                90,
                &local_time(2026, 10, 30, 14, 35, 12),
            );

            assert!(save_result.is_err());
        }
    }

    #[test]
    fn worker_posts_save_result_to_window() {
        let result_window = MessageOnlyWindow::create();
        let base_directory = unique_temporary_directory("save-worker-post");

        spawn_save_worker(
            three_by_two_test_image(),
            base_directory.clone(),
            GroupingMode::Yearly,
            SavedFormat::Jpg,
            90,
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
        assert!(base_directory.join("2026").join("2026-10-30_14-35-12.jpg").is_file());
    }
}
