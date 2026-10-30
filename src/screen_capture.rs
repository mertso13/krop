use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC, ROP_CODE, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::{Error, Result};

use crate::bgra_image::BYTES_PER_PIXEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualScreenBounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl VirtualScreenBounds {
    pub fn current() -> Self {
        unsafe {
            Self {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }
}

pub struct DibSection {
    bitmap: HBITMAP,
    top_down_bits: *mut u8,
    width: u32,
    height: u32,
}

impl DibSection {
    pub fn create(width: u32, height: u32) -> Result<Self> {
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut top_down_bits = std::ptr::null_mut();
        let bitmap = unsafe { CreateDIBSection(None, &bitmap_info, DIB_RGB_COLORS, &mut top_down_bits, None, 0)? };
        if top_down_bits.is_null() {
            unsafe {
                let _ = DeleteObject(bitmap.into());
            }
            return Err(Error::from_thread());
        }
        Ok(Self {
            bitmap,
            top_down_bits: top_down_bits.cast(),
            width,
            height,
        })
    }

    pub fn bitmap(&self) -> HBITMAP {
        self.bitmap
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.top_down_bits, self.pixel_byte_count()) }
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.top_down_bits, self.pixel_byte_count()) }
    }

    fn pixel_byte_count(&self) -> usize {
        self.width as usize * self.height as usize * BYTES_PER_PIXEL
    }
}

impl Drop for DibSection {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}

pub struct FrozenScreen {
    pub bounds: VirtualScreenBounds,
    pub bright: DibSection,
    pub dimmed: DibSection,
}

pub fn capture_virtual_screen() -> Result<FrozenScreen> {
    let bounds = VirtualScreenBounds::current();
    let bright = DibSection::create(bounds.width as u32, bounds.height as u32)?;
    unsafe {
        let screen_dc = GetDC(None::<HWND>);
        if screen_dc.is_invalid() {
            return Err(Error::from_thread());
        }
        let copy_result = copy_screen_into_dib_section(screen_dc, bounds, &bright);
        ReleaseDC(None, screen_dc);
        copy_result?;
    }
    let mut dimmed = DibSection::create(bright.width(), bright.height())?;
    darken_by_half(bright.pixels(), dimmed.pixels_mut());
    Ok(FrozenScreen { bounds, bright, dimmed })
}

unsafe fn copy_screen_into_dib_section(screen_dc: HDC, bounds: VirtualScreenBounds, target: &DibSection) -> Result<()> {
    unsafe {
        let memory_dc = CreateCompatibleDC(Some(screen_dc));
        if memory_dc.is_invalid() {
            return Err(Error::from_thread());
        }
        let previous_object = SelectObject(memory_dc, target.bitmap().into());
        let blit_result = BitBlt(
            memory_dc,
            0,
            0,
            bounds.width,
            bounds.height,
            Some(screen_dc),
            bounds.left,
            bounds.top,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        );
        SelectObject(memory_dc, previous_object);
        let _ = DeleteDC(memory_dc);
        let _ = GdiFlush();
        blit_result
    }
}

fn darken_by_half(bright_pixels: &[u8], dimmed_pixels: &mut [u8]) {
    for (dimmed_byte, bright_byte) in dimmed_pixels.iter_mut().zip(bright_pixels) {
        *dimmed_byte = bright_byte >> 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkening_halves_every_channel() {
        let bright_pixels = [200, 100, 51, 255, 0, 1, 2, 3];
        let mut dimmed_pixels = [0; 8];
        darken_by_half(&bright_pixels, &mut dimmed_pixels);
        assert_eq!(dimmed_pixels, [100, 50, 25, 127, 0, 0, 1, 1]);
    }

    #[test]
    fn dib_section_exposes_writable_top_down_pixels() {
        let mut dib_section = DibSection::create(3, 2).unwrap();
        assert_eq!(dib_section.pixels().len(), 3 * 2 * BYTES_PER_PIXEL);
        dib_section.pixels_mut().fill(7);
        assert!(dib_section.pixels().iter().all(|&byte| byte == 7));
    }

    #[test]
    fn virtual_screen_capture_matches_bounds_and_produces_dimmed_copy() {
        let frozen_screen = capture_virtual_screen().unwrap();
        let bounds = VirtualScreenBounds::current();

        assert_eq!(frozen_screen.bounds, bounds);
        assert_eq!(frozen_screen.bright.width(), bounds.width as u32);
        assert_eq!(frozen_screen.bright.height(), bounds.height as u32);
        assert!(
            frozen_screen
                .dimmed
                .pixels()
                .iter()
                .zip(frozen_screen.bright.pixels())
                .all(|(&dimmed_byte, &bright_byte)| dimmed_byte == bright_byte >> 1)
        );
    }
}
