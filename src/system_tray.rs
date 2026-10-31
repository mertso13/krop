use crate::autostart;
use crate::config::{Config, GroupingMode, SavedFormat};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, HICON, HMENU,
    IDI_APPLICATION, IMAGE_ICON, LR_SHARED, LoadIconW, LoadImageW, MF_CHECKED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SM_CYSMICON, SetForegroundWindow,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenu,
    WM_APP, WM_NULL,
};
use windows::core::{Error, PCWSTR, Result, w};

pub const WM_APP_SYSTEM_TRAY: u32 = WM_APP + 2;

const TRAY_ICON_ID: u32 = 1;
const TRAY_TOOLTIP_TEXT: &str = "krop - PrtSc to capture";
const MENU_OPEN_FOLDER: usize = 1;
const MENU_CHANGE_SAVE_FOLDER: usize = 2;
const MENU_GROUPING_DAILY: usize = 3;
const MENU_GROUPING_MONTHLY: usize = 4;
const MENU_GROUPING_YEARLY: usize = 5;
const MENU_SAVED_FORMAT_PNG: usize = 6;
const MENU_SAVED_FORMAT_JPG: usize = 7;
const MENU_START_WITH_WINDOWS: usize = 8;
const MENU_EXIT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    OpenFolder,
    ChangeSaveFolder,
    SetGroupingMode(GroupingMode),
    SetSavedFormat(SavedFormat),
    ToggleAutostart,
    Exit,
}

pub struct SystemTrayIcon {
    icon_data: NOTIFYICONDATAW,
}

pub fn add_tray_icon(window: HWND) -> Result<SystemTrayIcon> {
    let icon_data = tray_icon_data(window)?;
    if unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data) }.as_bool() {
        Ok(SystemTrayIcon { icon_data })
    } else {
        Err(Error::from_thread())
    }
}

impl Drop for SystemTrayIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.icon_data);
        }
    }
}

fn load_application_icon() -> Result<HICON> {
    let embedded_icon = unsafe {
        GetModuleHandleW(None).ok().and_then(|module_handle| {
            let icon_width = GetSystemMetrics(SM_CXSMICON);
            let icon_height = GetSystemMetrics(SM_CYSMICON);
            LoadImageW(
                Some(module_handle.into()),
                PCWSTR(std::ptr::without_provenance(1)),
                IMAGE_ICON,
                icon_width,
                icon_height,
                LR_SHARED,
            )
            .ok()
        })
    };
    match embedded_icon {
        Some(handle) if !handle.is_invalid() => Ok(HICON(handle.0)),
        _ => unsafe { LoadIconW(None, IDI_APPLICATION) },
    }
}

fn tray_icon_data(window: HWND) -> Result<NOTIFYICONDATAW> {
    let mut icon_data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_APP_SYSTEM_TRAY,
        hIcon: load_application_icon()?,
        ..Default::default()
    };
    let tooltip: Vec<u16> = TRAY_TOOLTIP_TEXT.encode_utf16().collect();
    icon_data.szTip[..tooltip.len()].copy_from_slice(&tooltip);
    Ok(icon_data)
}

pub fn show_context_menu(owner: HWND, config: &Config) -> Result<Option<TrayCommand>> {
    let menu = build_context_menu(config, autostart::is_autostart_enabled())?;
    let mut cursor_position = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor_position);
        let _ = SetForegroundWindow(owner);
    }
    let selected_menu_id = unsafe {
        TrackPopupMenu(
            menu.handle(),
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY | TPM_LEFTALIGN | TPM_TOPALIGN,
            cursor_position.x,
            cursor_position.y,
            None,
            owner,
            None,
        )
    }
    .0 as usize;
    unsafe {
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
    }
    Ok(command_for_menu_id(selected_menu_id))
}

struct ContextMenu(HMENU);

impl ContextMenu {
    fn create() -> Result<Self> {
        Ok(Self(unsafe { CreatePopupMenu()? }))
    }

    fn handle(&self) -> HMENU {
        self.0
    }
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

fn build_context_menu(config: &Config, start_with_windows_enabled: bool) -> Result<ContextMenu> {
    let menu = ContextMenu::create()?;
    unsafe {
        append_menu_item(menu.handle(), w!("Open Folder"), MENU_OPEN_FOLDER, false)?;
        append_menu_item(
            menu.handle(),
            w!("Change Save Folder..."),
            MENU_CHANGE_SAVE_FOLDER,
            false,
        )?;
        AppendMenuW(menu.handle(), MF_SEPARATOR, 0, PCWSTR::null())?;
        for (grouping_mode, label, menu_id) in grouping_mode_menu_entries() {
            append_menu_item(menu.handle(), label, menu_id, config.grouping_mode == grouping_mode)?;
        }
        AppendMenuW(menu.handle(), MF_SEPARATOR, 0, PCWSTR::null())?;
        append_saved_format_submenu(menu.handle(), config.saved_format)?;
        AppendMenuW(menu.handle(), MF_SEPARATOR, 0, PCWSTR::null())?;
        append_menu_item(
            menu.handle(),
            w!("Start with Windows"),
            MENU_START_WITH_WINDOWS,
            start_with_windows_enabled,
        )?;
        AppendMenuW(menu.handle(), MF_SEPARATOR, 0, PCWSTR::null())?;
        append_menu_item(menu.handle(), w!("Exit"), MENU_EXIT, false)?;
    }
    Ok(menu)
}

fn append_saved_format_submenu(menu: HMENU, saved_format: SavedFormat) -> Result<()> {
    let submenu = ContextMenu::create()?;
    unsafe {
        for (format_option, label, menu_id) in saved_format_menu_entries() {
            append_menu_item(
                submenu.handle(),
                label,
                menu_id,
                saved_format == format_option,
            )?;
        }
        AppendMenuW(
            menu,
            MF_POPUP,
            submenu.handle().0 as usize,
            w!("Saved Format"),
        )?;
    }
    std::mem::forget(submenu);
    Ok(())
}

unsafe fn append_menu_item(menu: HMENU, label: PCWSTR, menu_id: usize, checked: bool) -> Result<()> {
    unsafe {
        let menu_item_flags = if checked { MF_STRING | MF_CHECKED } else { MF_STRING };
        AppendMenuW(menu, menu_item_flags, menu_id, label)
    }
}

fn grouping_mode_menu_entries() -> [(GroupingMode, PCWSTR, usize); 3] {
    [
        (GroupingMode::Daily, w!("Daily"), MENU_GROUPING_DAILY),
        (GroupingMode::Monthly, w!("Monthly"), MENU_GROUPING_MONTHLY),
        (GroupingMode::Yearly, w!("Yearly"), MENU_GROUPING_YEARLY),
    ]
}

fn saved_format_menu_entries() -> [(SavedFormat, PCWSTR, usize); 2] {
    [
        (SavedFormat::Png, w!("PNG"), MENU_SAVED_FORMAT_PNG),
        (SavedFormat::Jpg, w!("JPG"), MENU_SAVED_FORMAT_JPG),
    ]
}

fn command_for_menu_id(menu_id: usize) -> Option<TrayCommand> {
    match menu_id {
        MENU_OPEN_FOLDER => Some(TrayCommand::OpenFolder),
        MENU_CHANGE_SAVE_FOLDER => Some(TrayCommand::ChangeSaveFolder),
        MENU_GROUPING_DAILY => Some(TrayCommand::SetGroupingMode(GroupingMode::Daily)),
        MENU_GROUPING_MONTHLY => Some(TrayCommand::SetGroupingMode(GroupingMode::Monthly)),
        MENU_GROUPING_YEARLY => Some(TrayCommand::SetGroupingMode(GroupingMode::Yearly)),
        MENU_SAVED_FORMAT_PNG => Some(TrayCommand::SetSavedFormat(SavedFormat::Png)),
        MENU_SAVED_FORMAT_JPG => Some(TrayCommand::SetSavedFormat(SavedFormat::Jpg)),
        MENU_START_WITH_WINDOWS => Some(TrayCommand::ToggleAutostart),
        MENU_EXIT => Some(TrayCommand::Exit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MessageOnlyWindow;
    use std::path::PathBuf;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuState, GetSubMenu, MF_BYCOMMAND,
    };

    fn test_config(grouping_mode: GroupingMode) -> Config {
        Config {
            base_directory: PathBuf::from(r"C:\Users\Example\Pictures\Screenshots"),
            grouping_mode,
            saved_format: SavedFormat::Png,
            jpeg_quality: 90,
        }
    }

    fn test_config_with_saved_format(saved_format: SavedFormat) -> Config {
        Config {
            saved_format,
            ..test_config(GroupingMode::Monthly)
        }
    }

    fn menu_item_is_checked(menu: HMENU, menu_id: usize) -> bool {
        let menu_state = unsafe { GetMenuState(menu, menu_id as u32, MF_BYCOMMAND) };
        assert_ne!(menu_state, u32::MAX, "menu item {menu_id} must exist");
        menu_state & MF_CHECKED.0 != 0
    }

    fn submenu_handle(menu: &ContextMenu) -> HMENU {
        let menu_item_count = unsafe { GetMenuItemCount(Some(menu.handle())) };
        let submenu_handles: Vec<HMENU> = (0..menu_item_count)
            .map(|position| unsafe { GetSubMenu(menu.handle(), position) })
            .filter(|submenu| !submenu.is_invalid())
            .collect();
        assert_eq!(submenu_handles.len(), 1, "expected exactly one tray submenu");
        submenu_handles[0]
    }

    #[test]
    fn context_menu_lists_every_english_tray_action() {
        let menu = build_context_menu(&test_config(GroupingMode::Monthly), false).unwrap();
        assert_eq!(unsafe { GetMenuItemCount(Some(menu.handle())) }, 12);
    }

    #[test]
    fn grouping_menu_shows_a_check_mark_only_for_the_active_mode() {
        for active_mode in [GroupingMode::Daily, GroupingMode::Monthly, GroupingMode::Yearly] {
            let menu = build_context_menu(&test_config(active_mode), false).unwrap();
            for (grouping_mode, _, menu_id) in grouping_mode_menu_entries() {
                assert_eq!(
                    menu_item_is_checked(menu.handle(), menu_id),
                    grouping_mode == active_mode,
                    "unexpected check state for {grouping_mode:?}"
                );
            }
        }
    }

    #[test]
    fn saved_format_submenu_shows_a_check_mark_only_for_the_active_format() {
        for active_format in [SavedFormat::Png, SavedFormat::Jpg] {
            let menu = build_context_menu(&test_config_with_saved_format(active_format), false).unwrap();
            let submenu = submenu_handle(&menu);
            for (saved_format, _, menu_id) in saved_format_menu_entries() {
                assert_eq!(
                    menu_item_is_checked(submenu, menu_id),
                    saved_format == active_format,
                    "unexpected check state for {saved_format:?}"
                );
            }
        }
    }

    #[test]
    fn start_with_windows_menu_item_reflects_the_autostart_state() {
        for start_with_windows_enabled in [false, true] {
            let menu = build_context_menu(&test_config(GroupingMode::Monthly), start_with_windows_enabled).unwrap();
            assert_eq!(
                menu_item_is_checked(menu.handle(), MENU_START_WITH_WINDOWS),
                start_with_windows_enabled
            );
        }
    }

    #[test]
    fn menu_ids_map_to_tray_commands() {
        assert_eq!(command_for_menu_id(MENU_OPEN_FOLDER), Some(TrayCommand::OpenFolder));
        assert_eq!(
            command_for_menu_id(MENU_CHANGE_SAVE_FOLDER),
            Some(TrayCommand::ChangeSaveFolder)
        );
        assert_eq!(
            command_for_menu_id(MENU_GROUPING_DAILY),
            Some(TrayCommand::SetGroupingMode(GroupingMode::Daily))
        );
        assert_eq!(
            command_for_menu_id(MENU_GROUPING_MONTHLY),
            Some(TrayCommand::SetGroupingMode(GroupingMode::Monthly))
        );
        assert_eq!(
            command_for_menu_id(MENU_GROUPING_YEARLY),
            Some(TrayCommand::SetGroupingMode(GroupingMode::Yearly))
        );
        assert_eq!(
            command_for_menu_id(MENU_SAVED_FORMAT_PNG),
            Some(TrayCommand::SetSavedFormat(SavedFormat::Png))
        );
        assert_eq!(
            command_for_menu_id(MENU_SAVED_FORMAT_JPG),
            Some(TrayCommand::SetSavedFormat(SavedFormat::Jpg))
        );
        assert_eq!(
            command_for_menu_id(MENU_START_WITH_WINDOWS),
            Some(TrayCommand::ToggleAutostart)
        );
        assert_eq!(command_for_menu_id(MENU_EXIT), Some(TrayCommand::Exit));
        assert_eq!(command_for_menu_id(0), None);
    }

    #[test]
    fn application_tray_icon_loads_successfully() {
        let icon = load_application_icon().unwrap();
        assert!(!icon.is_invalid());
        let default_icon = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap() };
        assert_ne!(icon, default_icon);
    }

    #[test]
    fn tray_icon_can_be_installed_and_removed() {
        let window = MessageOnlyWindow::create();
        {
            let tray_icon = add_tray_icon(window.handle()).unwrap();
            drop(tray_icon);
        }
        let _tray_icon = add_tray_icon(window.handle()).unwrap();
    }
}
