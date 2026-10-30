use crate::bgra_image::{BYTES_PER_PIXEL, BgraImage};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_DIB;
use windows::core::{Error, Result};

const OPEN_CLIPBOARD_ATTEMPTS: u32 = 10;
const OPEN_CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(10);
const OPAQUE_ALPHA: u8 = 0xFF;

pub fn copy_image_to_clipboard(clipboard_owner: HWND, image: &BgraImage) -> Result<()> {
    let device_independent_bitmap = build_bottom_up_device_independent_bitmap(image);
    open_clipboard_with_retry(clipboard_owner)?;
    let placement_result = unsafe { place_device_independent_bitmap_on_open_clipboard(&device_independent_bitmap) };
    unsafe {
        let _ = CloseClipboard();
    }
    placement_result
}

fn build_bottom_up_device_independent_bitmap(image: &BgraImage) -> Vec<u8> {
    let header_size = size_of::<BITMAPINFOHEADER>();
    let pixel_data_size = image.top_down_pixels().len();
    let header = BITMAPINFOHEADER {
        biSize: header_size as u32,
        biWidth: image.width() as i32,
        biHeight: image.height() as i32,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: pixel_data_size as u32,
        ..Default::default()
    };
    let mut device_independent_bitmap = Vec::with_capacity(header_size + pixel_data_size);
    device_independent_bitmap.extend_from_slice(unsafe {
        std::slice::from_raw_parts((&header as *const BITMAPINFOHEADER).cast::<u8>(), header_size)
    });
    for top_down_row in image.top_down_pixels().chunks_exact(image.row_stride()).rev() {
        for pixel in top_down_row.as_chunks::<BYTES_PER_PIXEL>().0 {
            device_independent_bitmap.extend_from_slice(&[pixel[0], pixel[1], pixel[2], OPAQUE_ALPHA]);
        }
    }
    device_independent_bitmap
}

fn open_clipboard_with_retry(clipboard_owner: HWND) -> Result<()> {
    let mut attempt_number = 1;
    loop {
        match unsafe { OpenClipboard(Some(clipboard_owner)) } {
            Ok(()) => return Ok(()),
            Err(error) if attempt_number >= OPEN_CLIPBOARD_ATTEMPTS => return Err(error),
            Err(_) => {
                attempt_number += 1;
                thread::sleep(OPEN_CLIPBOARD_RETRY_DELAY);
            }
        }
    }
}

unsafe fn place_device_independent_bitmap_on_open_clipboard(device_independent_bitmap: &[u8]) -> Result<()> {
    unsafe {
        EmptyClipboard()?;
        let global_memory = GlobalAlloc(GMEM_MOVEABLE, device_independent_bitmap.len())?;
        let locked_destination = GlobalLock(global_memory);
        if locked_destination.is_null() {
            let lock_error = Error::from_thread();
            let _ = GlobalFree(Some(global_memory));
            return Err(lock_error);
        }
        std::ptr::copy_nonoverlapping(
            device_independent_bitmap.as_ptr(),
            locked_destination.cast::<u8>(),
            device_independent_bitmap.len(),
        );
        let _ = GlobalUnlock(global_memory);
        if let Err(set_error) = SetClipboardData(u32::from(CF_DIB.0), Some(HANDLE(global_memory.0))) {
            let _ = GlobalFree(Some(global_memory));
            return Err(set_error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MessageOnlyWindow;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::GetClipboardData;
    use windows::Win32::System::Memory::GlobalSize;

    fn two_by_two_test_image() -> BgraImage {
        BgraImage::new(
            2,
            2,
            vec![
                1, 2, 3, 0, 4, 5, 6, 0,
                7, 8, 9, 0, 10, 11, 12, 0,
            ],
        )
    }

    #[test]
    fn device_independent_bitmap_is_bottom_up_and_opaque() {
        let device_independent_bitmap = build_bottom_up_device_independent_bitmap(&two_by_two_test_image());
        let header_size = size_of::<BITMAPINFOHEADER>();
        let header = unsafe { std::ptr::read_unaligned(device_independent_bitmap.as_ptr().cast::<BITMAPINFOHEADER>()) };

        assert_eq!(header.biWidth, 2);
        assert_eq!(header.biHeight, 2);
        assert_eq!(header.biBitCount, 32);
        assert_eq!(header.biCompression, BI_RGB.0);
        assert_eq!(
            &device_independent_bitmap[header_size..],
            &[7, 8, 9, 255, 10, 11, 12, 255, 1, 2, 3, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn clipboard_image_can_be_written_and_read_back() {
        let clipboard_owner = MessageOnlyWindow::create();
        let image = two_by_two_test_image();

        copy_image_to_clipboard(clipboard_owner.handle(), &image).unwrap();

        open_clipboard_with_retry(clipboard_owner.handle()).unwrap();
        let clipboard_bytes = unsafe {
            let clipboard_handle = GetClipboardData(u32::from(CF_DIB.0)).unwrap();
            let global_memory = HGLOBAL(clipboard_handle.0);
            let locked_source = GlobalLock(global_memory);
            assert!(!locked_source.is_null());
            let copied_bytes =
                std::slice::from_raw_parts(locked_source.cast::<u8>(), GlobalSize(global_memory)).to_vec();
            let _ = GlobalUnlock(global_memory);
            let _ = CloseClipboard();
            copied_bytes
        };

        let expected_bytes = build_bottom_up_device_independent_bitmap(&image);
        assert_eq!(&clipboard_bytes[..expected_bytes.len()], expected_bytes.as_slice());
    }
}
