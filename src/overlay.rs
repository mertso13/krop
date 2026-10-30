use crate::bgra_image::BgraImage;
use crate::screen_capture::{DibSection, FrozenScreen, VirtualScreenBounds};
use crate::selection::{crop_bgra_region, is_selection_large_enough, normalized_selection_rect};
use std::borrow::Borrow;
use windows::Win32::Foundation::{
    COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BLACK_BRUSH, BeginPaint, BitBlt, CreateCompatibleDC, DEFAULT_GUI_FONT, DT_CALCRECT, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DeleteDC, DrawTextW, EndPaint, FillRect, GET_STOCK_OBJECT_FLAGS, GetStockObject,
    HBRUSH, HDC, HGDIOBJ, InvalidateRect, PAINTSTRUCT, SRCCOPY, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    UpdateWindow, WHITE_BRUSH,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, IDC_CROSS, LoadCursorW, MSG, RegisterClassW, SW_SHOW,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, WNDCLASSW, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result, w};

const OVERLAY_WINDOW_CLASS: PCWSTR = w!("KropRegionSelectionOverlay");
const OVERLAY_WINDOW_TITLE: PCWSTR = w!("krop region selection");
const BORDER_THICKNESS: i32 = 1;
const LABEL_MARGIN: i32 = 4;
const LABEL_HORIZONTAL_PADDING: i32 = 6;
const LABEL_VERTICAL_PADDING: i32 = 2;
const LABEL_TEXT_COLOR: COLORREF = COLORREF(0x00FF_FFFF);

pub fn select_region(frozen_screen: FrozenScreen) -> Result<Option<BgraImage>> {
    let bounds = frozen_screen.bounds;
    let mut state = Box::new(OverlayState::new(frozen_screen)?);
    let state_pointer: *mut OverlayState = &mut *state;
    let window = create_overlay_window(bounds, state_pointer)?;
    unsafe {
        let _ = SetForegroundWindow(window);
        let _ = SetFocus(Some(window));
        let _ = ShowWindow(window, SW_SHOW);
        let _ = UpdateWindow(window);
    }
    run_overlay_message_loop(state_pointer);
    unsafe {
        let _ = DestroyWindow(window);
        Ok((*state_pointer).cropped_image.take())
    }
}

struct OverlayState {
    painter: OverlayPainter,
    pointer_position: POINT,
    selection_anchor: Option<POINT>,
    cropped_image: Option<BgraImage>,
    finished: bool,
}

impl OverlayState {
    fn new(frozen_screen: FrozenScreen) -> Result<Self> {
        Ok(Self {
            painter: OverlayPainter::new(frozen_screen)?,
            pointer_position: POINT::default(),
            selection_anchor: None,
            cropped_image: None,
            finished: false,
        })
    }

    fn screen_width(&self) -> i32 {
        self.painter.screen_width
    }

    fn screen_height(&self) -> i32 {
        self.painter.screen_height
    }

    fn selection_rectangle(&self) -> Option<RECT> {
        self.selection_anchor.map(|anchor| {
            normalized_selection_rect(anchor, self.pointer_position, self.screen_width(), self.screen_height())
        })
    }

    fn complete_selection(&mut self) {
        let valid_selection = self.selection_rectangle().filter(is_selection_large_enough);
        if let Some(selection) = valid_selection {
            self.cropped_image = crop_bgra_region(
                self.painter.bright.bitmap.pixels(),
                self.screen_width() as u32,
                self.screen_height() as u32,
                &selection,
            );
        }
        self.finished = true;
    }

    fn cancel_selection(&mut self) {
        self.cropped_image = None;
        self.finished = true;
    }
}

struct OverlayPainter {
    screen_width: i32,
    screen_height: i32,
    dimmed: BitmapContext<DibSection>,
    bright: BitmapContext<DibSection>,
    back_buffer: BitmapContext<DibSection>,
}

impl OverlayPainter {
    fn new(frozen_screen: FrozenScreen) -> Result<Self> {
        let screen_width = frozen_screen.bright.width() as i32;
        let screen_height = frozen_screen.bright.height() as i32;
        let dimmed = BitmapContext::new(frozen_screen.dimmed)?;
        let bright = BitmapContext::new(frozen_screen.bright)?;
        let back_buffer = BitmapContext::new(DibSection::create(screen_width as u32, screen_height as u32)?)?;
        Ok(Self {
            screen_width,
            screen_height,
            dimmed,
            bright,
            back_buffer,
        })
    }

    fn paint(&self, window_dc: HDC, selection: Option<&RECT>) {
        unsafe {
            let _ = BitBlt(
                self.back_buffer.device_context,
                0,
                0,
                self.screen_width,
                self.screen_height,
                Some(self.dimmed.device_context),
                0,
                0,
                SRCCOPY,
            );
            if let Some(selection) = selection.filter(|selection| rectangle_has_area(selection)) {
                let selection_width = selection.right - selection.left;
                let selection_height = selection.bottom - selection.top;
                let _ = BitBlt(
                    self.back_buffer.device_context,
                    selection.left,
                    selection.top,
                    selection_width,
                    selection_height,
                    Some(self.bright.device_context),
                    selection.left,
                    selection.top,
                    SRCCOPY,
                );
                draw_selection_border(self.back_buffer.device_context, selection);
                draw_dimension_label(
                    self.back_buffer.device_context,
                    selection,
                    self.screen_width,
                    self.screen_height,
                );
            }
            let _ = BitBlt(
                window_dc,
                0,
                0,
                self.screen_width,
                self.screen_height,
                Some(self.back_buffer.device_context),
                0,
                0,
                SRCCOPY,
            );
        }
    }
}

struct BitmapContext<B> {
    device_context: HDC,
    previous_bitmap: HGDIOBJ,
    bitmap: B,
}

impl<B: Borrow<DibSection>> BitmapContext<B> {
    fn new(bitmap: B) -> Result<Self> {
        unsafe {
            let device_context = CreateCompatibleDC(None);
            if device_context.is_invalid() {
                return Err(Error::from_thread());
            }
            let previous_bitmap = SelectObject(device_context, bitmap.borrow().bitmap().into());
            Ok(Self {
                device_context,
                previous_bitmap,
                bitmap,
            })
        }
    }
}

impl<B> Drop for BitmapContext<B> {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.device_context, self.previous_bitmap);
            let _ = DeleteDC(self.device_context);
        }
    }
}

fn draw_selection_border(device_context: HDC, selection: &RECT) {
    fill_rectangle_outline(device_context, selection, BORDER_THICKNESS, stock_brush(WHITE_BRUSH));
    let inner_selection = RECT {
        left: selection.left + BORDER_THICKNESS,
        top: selection.top + BORDER_THICKNESS,
        right: selection.right - BORDER_THICKNESS,
        bottom: selection.bottom - BORDER_THICKNESS,
    };
    fill_rectangle_outline(device_context, &inner_selection, BORDER_THICKNESS, stock_brush(BLACK_BRUSH));
}

fn draw_dimension_label(device_context: HDC, selection: &RECT, screen_width: i32, screen_height: i32) {
    let mut label_text: Vec<u16> = format!("{} x {}", selection.right - selection.left, selection.bottom - selection.top)
        .encode_utf16()
        .collect();
    unsafe {
        let previous_font = SelectObject(device_context, GetStockObject(DEFAULT_GUI_FONT));
        SetBkMode(device_context, TRANSPARENT);
        let mut text_size = RECT::default();
        DrawTextW(
            device_context,
            &mut label_text,
            &mut text_size,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        let mut label_rectangle = dimension_label_rect(
            selection,
            text_size.right - text_size.left,
            text_size.bottom - text_size.top,
            screen_width,
            screen_height,
        );
        if rectangle_has_area(&label_rectangle) {
            FillRect(device_context, &label_rectangle, stock_brush(BLACK_BRUSH));
            SetTextColor(device_context, LABEL_TEXT_COLOR);
            DrawTextW(
                device_context,
                &mut label_text,
                &mut label_rectangle,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
        SelectObject(device_context, previous_font);
    }
}

fn dimension_label_rect(
    selection: &RECT,
    text_width: i32,
    text_height: i32,
    screen_width: i32,
    screen_height: i32,
) -> RECT {
    let label_width = text_width + 2 * LABEL_HORIZONTAL_PADDING;
    let label_height = text_height + 2 * LABEL_VERTICAL_PADDING;
    let left = selection
        .left
        .clamp(0, (screen_width - label_width).max(0));
    let top = if selection.top >= label_height + LABEL_MARGIN {
        selection.top - label_height - LABEL_MARGIN
    } else {
        selection.top + LABEL_MARGIN
    }
    .clamp(0, (screen_height - label_height).max(0));
    RECT {
        left,
        top,
        right: left + label_width,
        bottom: top + label_height,
    }
}

fn fill_rectangle_outline(device_context: HDC, rectangle: &RECT, thickness: i32, brush: HBRUSH) {
    if !rectangle_has_area(rectangle) || thickness <= 0 || thickness * 2 > rectangle.right - rectangle.left
        || thickness * 2 > rectangle.bottom - rectangle.top
    {
        return;
    }
    let edge_rectangles = [
        RECT {
            left: rectangle.left,
            top: rectangle.top,
            right: rectangle.right,
            bottom: rectangle.top + thickness,
        },
        RECT {
            left: rectangle.left,
            top: rectangle.bottom - thickness,
            right: rectangle.right,
            bottom: rectangle.bottom,
        },
        RECT {
            left: rectangle.left,
            top: rectangle.top,
            right: rectangle.left + thickness,
            bottom: rectangle.bottom,
        },
        RECT {
            left: rectangle.right - thickness,
            top: rectangle.top,
            right: rectangle.right,
            bottom: rectangle.bottom,
        },
    ];
    for edge_rectangle in &edge_rectangles {
        unsafe {
            FillRect(device_context, edge_rectangle, brush);
        }
    }
}

fn rectangle_has_area(rectangle: &RECT) -> bool {
    rectangle.right > rectangle.left && rectangle.bottom > rectangle.top
}

fn stock_brush(stock_object: GET_STOCK_OBJECT_FLAGS) -> HBRUSH {
    unsafe { HBRUSH(GetStockObject(stock_object).0) }
}

fn create_overlay_window(bounds: VirtualScreenBounds, state_pointer: *mut OverlayState) -> Result<HWND> {
    let module = unsafe { GetModuleHandleW(None) }?;
    let instance = HINSTANCE(module.0);
    register_overlay_window_class(instance)?;
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            OVERLAY_WINDOW_CLASS,
            OVERLAY_WINDOW_TITLE,
            WS_POPUP,
            bounds.left,
            bounds.top,
            bounds.width,
            bounds.height,
            None,
            None,
            Some(instance),
            Some(state_pointer.cast()),
        )
    }
}

fn register_overlay_window_class(instance: HINSTANCE) -> Result<()> {
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(overlay_window_procedure),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_CROSS).unwrap_or_default() },
        lpszClassName: OVERLAY_WINDOW_CLASS,
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

fn run_overlay_message_loop(state_pointer: *mut OverlayState) {
    let mut message = MSG::default();
    loop {
        let message_result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if message_result.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if unsafe { (*state_pointer).finished } {
            break;
        }
    }
}

unsafe extern "system" fn overlay_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create_struct = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create_struct.lpCreateParams as isize);
            return LRESULT(1);
        }

        let state_pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut OverlayState;
        if state_pointer.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let state = &mut *state_pointer;

        match message {
            WM_PAINT => {
                let mut paint_struct = PAINTSTRUCT::default();
                let window_dc = BeginPaint(window, &mut paint_struct);
                let selection = state.selection_rectangle();
                state.painter.paint(window_dc, selection.as_ref());
                let _ = EndPaint(window, &paint_struct);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MOUSEMOVE => {
                state.pointer_position = client_point(lparam);
                if state.selection_anchor.is_some() {
                    let _ = InvalidateRect(Some(window), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                state.pointer_position = client_point(lparam);
                state.selection_anchor = Some(state.pointer_position);
                let _ = SetCapture(window);
                let _ = InvalidateRect(Some(window), None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                state.pointer_position = client_point(lparam);
                let _ = ReleaseCapture();
                state.complete_selection();
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_RBUTTONDOWN => {
                let _ = ReleaseCapture();
                state.cancel_selection();
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u16 == VK_ESCAPE.0 {
                    let _ = ReleaseCapture();
                    state.cancel_selection();
                    let _ = DestroyWindow(window);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

fn client_point(lparam: LPARAM) -> POINT {
    let packed_position = lparam.0 as u32;
    POINT {
        x: (packed_position & 0xFFFF) as u16 as i16 as i32,
        y: ((packed_position >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetClientRect, SendMessageW, WS_CAPTION,
    };

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    fn create_test_overlay() -> (Box<OverlayState>, HWND, VirtualScreenBounds) {
        let frozen_screen = crate::screen_capture::capture_virtual_screen().unwrap();
        let bounds = frozen_screen.bounds;
        let mut state = Box::new(OverlayState::new(frozen_screen).unwrap());
        let window = create_overlay_window(bounds, &mut *state as *mut OverlayState).unwrap();
        (state, window, bounds)
    }

    fn packed_point(x: i32, y: i32) -> LPARAM {
        LPARAM((((y as u32) & 0xFFFF) << 16 | ((x as u32) & 0xFFFF)) as isize)
    }

    fn send_mouse_message(window: HWND, message: u32, x: i32, y: i32) {
        unsafe {
            let _ = SendMessageW(window, message, Some(WPARAM(0)), Some(packed_point(x, y)));
        }
    }

    #[test]
    fn client_point_unpacks_both_axes_with_sign() {
        assert_eq!(client_point(LPARAM(((200i32 << 16) | 100i32) as isize)), POINT { x: 100, y: 200 });
        let negative_lparam = (((-2i32 as u32) & 0xFFFF) << 16 | ((-1i32 as u32) & 0xFFFF)) as isize;
        assert_eq!(client_point(LPARAM(negative_lparam)), POINT { x: -1, y: -2 });
    }

    #[test]
    fn dimension_label_sits_above_the_selection_when_there_is_room() {
        let label = dimension_label_rect(&rect(10, 100, 110, 200), 40, 16, 800, 600);
        assert_eq!(label, rect(10, 76, 62, 96));
    }

    #[test]
    fn dimension_label_drops_inside_the_selection_near_the_top_edge() {
        let label = dimension_label_rect(&rect(10, 2, 110, 200), 40, 16, 800, 600);
        assert_eq!(label, rect(10, 6, 62, 26));
    }

    #[test]
    fn dimension_label_is_clamped_inside_the_screen() {
        let label = dimension_label_rect(&rect(780, 500, 800, 600), 40, 16, 800, 600);
        assert_eq!(label, rect(748, 476, 800, 496));
    }

    #[test]
    fn empty_rectangles_have_no_area() {
        assert!(!rectangle_has_area(&rect(10, 10, 10, 40)));
        assert!(!rectangle_has_area(&rect(10, 10, 40, 10)));
        assert!(rectangle_has_area(&rect(10, 10, 40, 40)));
    }

    #[test]
    fn overlay_window_is_borderless_topmost_and_covers_the_virtual_screen() {
        for _ in 0..2 {
            let (_state, window, bounds) = create_test_overlay();

            unsafe {
                let extended_style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
                let window_style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
                let mut client_rectangle = RECT::default();
                GetClientRect(window, &mut client_rectangle).unwrap();

                assert_eq!(extended_style & WS_EX_TOPMOST.0, WS_EX_TOPMOST.0);
                assert_eq!(extended_style & WS_EX_TOOLWINDOW.0, WS_EX_TOOLWINDOW.0);
                assert_eq!(window_style & WS_POPUP.0, WS_POPUP.0);
                assert_eq!(window_style & WS_CAPTION.0, 0);
                assert_eq!(client_rectangle.right - client_rectangle.left, bounds.width);
                assert_eq!(client_rectangle.bottom - client_rectangle.top, bounds.height);

                let _ = DestroyWindow(window);
            }
        }
    }

    #[test]
    fn left_button_release_crops_the_dragged_region_in_any_direction() {
        for (anchor, release) in [
            ((10, 20), (50, 60)),
            ((50, 60), (10, 20)),
            ((50, 20), (10, 60)),
            ((10, 60), (50, 20)),
        ] {
            let (state, window, _bounds) = create_test_overlay();

            send_mouse_message(window, WM_LBUTTONDOWN, anchor.0, anchor.1);
            send_mouse_message(window, WM_MOUSEMOVE, release.0, release.1);
            send_mouse_message(window, WM_LBUTTONUP, release.0, release.1);

            assert!(state.finished);
            let cropped_image = state.cropped_image.as_ref().unwrap();
            assert_eq!((cropped_image.width(), cropped_image.height()), (40, 40));
        }
    }

    #[test]
    fn escape_and_right_click_cancel_without_cropping() {
        for (cancel_message, cancel_wparam) in [
            (WM_KEYDOWN, WPARAM(VK_ESCAPE.0 as usize)),
            (WM_RBUTTONDOWN, WPARAM(0)),
        ] {
            let (state, window, _bounds) = create_test_overlay();

            send_mouse_message(window, WM_LBUTTONDOWN, 10, 20);
            send_mouse_message(window, WM_MOUSEMOVE, 50, 60);
            unsafe {
                let _ = SendMessageW(window, cancel_message, Some(cancel_wparam), Some(packed_point(50, 60)));
            }

            assert!(state.finished);
            assert!(state.cropped_image.is_none());
        }
    }

    #[test]
    fn tiny_click_cancels_without_cropping() {
        let (state, window, _bounds) = create_test_overlay();

        send_mouse_message(window, WM_LBUTTONDOWN, 10, 20);
        send_mouse_message(window, WM_LBUTTONUP, 13, 23);

        assert!(state.finished);
        assert!(state.cropped_image.is_none());
    }
}
