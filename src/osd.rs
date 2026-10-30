use std::ffi::c_void;
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DEFAULT_GUI_FONT, DT_CALCRECT, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetStockObject, HDC, PAINTSTRUCT,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, LWA_ALPHA,
    RegisterClassW, SPI_GETWORKAREA, SW_SHOWNA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetLayeredWindowAttributes,
    SetTimer, SetWindowDisplayAffinity, SetWindowLongPtrW, ShowWindow, SystemParametersInfoW,
    WDA_EXCLUDEFROMCAPTURE, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result, w};

const OSD_WINDOW_CLASS: PCWSTR = w!("KropMiniOsdPill");
const OSD_WINDOW_TITLE: PCWSTR = w!("krop feedback");
const AUTO_HIDE_TIMER_ID: usize = 1;
const AUTO_HIDE_MILLISECONDS: u32 = 1_200;
const PILL_HORIZONTAL_PADDING: i32 = 18;
const PILL_VERTICAL_PADDING: i32 = 10;
const PILL_BOTTOM_MARGIN: i32 = 48;
const PILL_CORNER_DIAMETER: i32 = 24;
const PILL_ALPHA: u8 = 235;
const TEXT_COLOR: COLORREF = COLORREF(0x00FF_FFFF);
const SUCCESS_BACKGROUND: COLORREF = COLORREF(0x001F_1F1F);
const ERROR_BACKGROUND: COLORREF = COLORREF(0x001E_1E5A);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsdFeedback {
    CaptureSaved,
    SaveFailed,
}

impl OsdFeedback {
    fn label_text(self) -> &'static str {
        match self {
            Self::CaptureSaved => "Copied to clipboard  ·  Saved to disk",
            Self::SaveFailed => "Copied to clipboard  ·  Saving to disk failed",
        }
    }

    fn background_color(self) -> COLORREF {
        match self {
            Self::CaptureSaved => SUCCESS_BACKGROUND,
            Self::SaveFailed => ERROR_BACKGROUND,
        }
    }
}

pub fn show_feedback(feedback: OsdFeedback) -> Result<HWND> {
    let (pill_width, pill_height) = measure_pill(feedback);
    let (pill_x, pill_y) = feedback_window_position(&primary_work_area()?, pill_width, pill_height);
    let instance = unsafe { HINSTANCE(GetModuleHandleW(None)?.0) };
    register_osd_window_class(instance)?;
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            OSD_WINDOW_CLASS,
            OSD_WINDOW_TITLE,
            WS_POPUP,
            pill_x,
            pill_y,
            pill_width,
            pill_height,
            None,
            None,
            Some(instance),
            None,
        )?
    };
    if let Err(configuration_error) = configure_feedback_window(window, pill_width, pill_height, feedback) {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(configuration_error);
    }
    Ok(window)
}

fn configure_feedback_window(
    window: HWND,
    pill_width: i32,
    pill_height: i32,
    feedback: OsdFeedback,
) -> Result<()> {
    let state_pointer = Box::into_raw(Box::new(FeedbackState { feedback }));
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, state_pointer as isize);
        SetLayeredWindowAttributes(window, COLORREF(0), PILL_ALPHA, LWA_ALPHA)?;
        let window_region = CreateRoundRectRgn(
            0,
            0,
            pill_width + 1,
            pill_height + 1,
            PILL_CORNER_DIAMETER,
            PILL_CORNER_DIAMETER,
        );
        if SetWindowRgn(window, Some(window_region), true) == 0 {
            let _ = DeleteObject(window_region.into());
            return Err(Error::from_thread());
        }
        let _ = ShowWindow(window, SW_SHOWNA);
        SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE)?;
        if SetTimer(Some(window), AUTO_HIDE_TIMER_ID, AUTO_HIDE_MILLISECONDS, None) == 0 {
            return Err(Error::from_thread());
        }
        let _ = UpdateWindow(window);
    }
    Ok(())
}

fn release_feedback_state(window: HWND) {
    unsafe {
        let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut FeedbackState;
        if !state_pointer.is_null() {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_pointer));
        }
    }
}

struct FeedbackState {
    feedback: OsdFeedback,
}

fn measure_pill(feedback: OsdFeedback) -> (i32, i32) {
    let mut label_text: Vec<u16> = feedback.label_text().encode_utf16().collect();
    let mut label_rectangle = RECT::default();
    unsafe {
        let screen_device_context = GetDC(None::<HWND>);
        let previous_font = SelectObject(screen_device_context, GetStockObject(DEFAULT_GUI_FONT));
        DrawTextW(
            screen_device_context,
            &mut label_text,
            &mut label_rectangle,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(screen_device_context, previous_font);
        let _ = ReleaseDC(None, screen_device_context);
    }
    (
        label_rectangle.right - label_rectangle.left + 2 * PILL_HORIZONTAL_PADDING,
        label_rectangle.bottom - label_rectangle.top + 2 * PILL_VERTICAL_PADDING,
    )
}

fn paint_pill(device_context: HDC, client_rectangle: &RECT, feedback: OsdFeedback) {
    unsafe {
        let background_brush = CreateSolidBrush(feedback.background_color());
        FillRect(device_context, client_rectangle, background_brush);
        let _ = DeleteObject(background_brush.into());
        let previous_font = SelectObject(device_context, GetStockObject(DEFAULT_GUI_FONT));
        SetBkMode(device_context, TRANSPARENT);
        SetTextColor(device_context, TEXT_COLOR);
        let mut label_text: Vec<u16> = feedback.label_text().encode_utf16().collect();
        let mut label_rectangle = *client_rectangle;
        DrawTextW(
            device_context,
            &mut label_text,
            &mut label_rectangle,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(device_context, previous_font);
    }
}

fn feedback_window_position(work_area: &RECT, pill_width: i32, pill_height: i32) -> (i32, i32) {
    let centered_x = work_area.left + (work_area.right - work_area.left - pill_width) / 2;
    let bottom_y = work_area.bottom - pill_height - PILL_BOTTOM_MARGIN;
    (
        centered_x.max(work_area.left),
        bottom_y.max(work_area.top),
    )
}

fn primary_work_area() -> Result<RECT> {
    let mut work_area = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )?;
    }
    Ok(work_area)
}

fn register_osd_window_class(instance: HINSTANCE) -> Result<()> {
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(osd_window_procedure),
        hInstance: instance,
        lpszClassName: OSD_WINDOW_CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        let registration_error = unsafe { GetLastError() };
        if registration_error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

unsafe extern "system" fn osd_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_NCDESTROY => {
                release_feedback_state(window);
                DefWindowProcW(window, message, wparam, lparam)
            }
            WM_PAINT => {
                let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const FeedbackState;
                if state_pointer.is_null() {
                    return DefWindowProcW(window, message, wparam, lparam);
                }
                let mut paint_struct = PAINTSTRUCT::default();
                let window_device_context = BeginPaint(window, &mut paint_struct);
                let mut client_rectangle = RECT::default();
                let _ = GetClientRect(window, &mut client_rectangle);
                paint_pill(
                    window_device_context,
                    &client_rectangle,
                    (*state_pointer).feedback,
                );
                let _ = EndPaint(window, &paint_struct);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_TIMER => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_capture::DibSection;
    use std::thread;
    use std::time::{Duration, Instant};
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GWL_EXSTYLE, GetWindowDisplayAffinity, GetWindowRect, IsWindow, IsWindowVisible, MSG,
        PM_REMOVE, PeekMessageW, SendMessageW, TranslateMessage,
    };

    fn render_pill(feedback: OsdFeedback) -> (DibSection, i32, i32) {
        let (pill_width, pill_height) = measure_pill(feedback);
        let dib_section = DibSection::create(pill_width as u32, pill_height as u32).unwrap();
        let client_rectangle = RECT {
            left: 0,
            top: 0,
            right: pill_width,
            bottom: pill_height,
        };
        unsafe {
            let device_context = CreateCompatibleDC(None);
            let previous_bitmap = SelectObject(device_context, dib_section.bitmap().into());
            paint_pill(device_context, &client_rectangle, feedback);
            SelectObject(device_context, previous_bitmap);
            let _ = DeleteDC(device_context);
        }
        (dib_section, pill_width, pill_height)
    }

    fn expected_bgr(color: COLORREF) -> [u8; 3] {
        [
            ((color.0 >> 16) & 0xFF) as u8,
            ((color.0 >> 8) & 0xFF) as u8,
            (color.0 & 0xFF) as u8,
        ]
    }

    #[test]
    fn success_and_failure_feedback_text_is_explicit() {
        let saved_text = OsdFeedback::CaptureSaved.label_text();
        assert!(saved_text.contains("Copied to clipboard"));
        assert!(saved_text.contains("Saved to disk"));

        let failed_text = OsdFeedback::SaveFailed.label_text();
        assert!(failed_text.contains("Copied to clipboard"));
        assert!(failed_text.contains("failed"));
    }

    #[test]
    fn pill_is_centered_above_the_bottom_of_the_work_area() {
        assert_eq!(
            feedback_window_position(
                &RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1040,
                },
                240,
                40,
            ),
            (840, 952)
        );
        assert_eq!(
            feedback_window_position(
                &RECT {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1080,
                },
                240,
                40,
            ),
            (-1080, 992)
        );
    }

    #[test]
    fn success_and_failure_pills_paint_distinct_backgrounds_and_labels() {
        for (feedback, background) in [
            (OsdFeedback::CaptureSaved, SUCCESS_BACKGROUND),
            (OsdFeedback::SaveFailed, ERROR_BACKGROUND),
        ] {
            let (dib_section, width, height) = render_pill(feedback);
            assert_eq!((dib_section.width(), dib_section.height()), (width as u32, height as u32));

            let expected_background = expected_bgr(background);
            let pixels = dib_section.pixels();
            assert_eq!(&pixels[..3], &expected_background);
            assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[..3] != expected_background));
        }
    }

    #[test]
    fn pill_window_has_required_styles_affinity_and_auto_hide() {
        for feedback in [OsdFeedback::CaptureSaved, OsdFeedback::SaveFailed] {
            let window = show_feedback(feedback).unwrap();
            let (pill_width, pill_height) = measure_pill(feedback);
            let (expected_x, expected_y) =
                feedback_window_position(&primary_work_area().unwrap(), pill_width, pill_height);

            unsafe {
                let extended_style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
                for required_style in [
                    WS_EX_TOPMOST,
                    WS_EX_TOOLWINDOW,
                    WS_EX_NOACTIVATE,
                    WS_EX_LAYERED,
                    WS_EX_TRANSPARENT,
                ] {
                    assert_eq!(extended_style & required_style.0, required_style.0);
                }

                let mut display_affinity = 0u32;
                GetWindowDisplayAffinity(window, &mut display_affinity).unwrap();
                assert_eq!(display_affinity, WDA_EXCLUDEFROMCAPTURE.0);
                assert!(IsWindowVisible(window).as_bool());

                let mut window_rectangle = RECT::default();
                GetWindowRect(window, &mut window_rectangle).unwrap();
                assert_eq!(window_rectangle.right - window_rectangle.left, pill_width);
                assert_eq!(window_rectangle.bottom - window_rectangle.top, pill_height);
                assert!((window_rectangle.left - expected_x).abs() <= 1);
                assert!((window_rectangle.top - expected_y).abs() <= 1);

                let _ = SendMessageW(
                    window,
                    WM_TIMER,
                    Some(WPARAM(AUTO_HIDE_TIMER_ID)),
                    Some(LPARAM(0)),
                );
                assert!(!IsWindow(Some(window)).as_bool());
            }
        }
    }

    #[test]
    fn pill_destroys_itself_after_the_auto_hide_delay() {
        let window = show_feedback(OsdFeedback::CaptureSaved).unwrap();
        let show_time = Instant::now();
        let deadline = show_time + Duration::from_secs(5);
        while unsafe { IsWindow(Some(window)).as_bool() } && Instant::now() < deadline {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(!unsafe { IsWindow(Some(window)).as_bool() });
        assert!(show_time.elapsed() >= Duration::from_millis(AUTO_HIDE_MILLISECONDS as u64));
    }
}
