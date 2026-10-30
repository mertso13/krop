use std::ffi::c_void;
use std::path::PathBuf;
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::core::Result;

pub fn pick_folder(owner: HWND) -> Result<Option<PathBuf>> {
    unsafe {
        let folder_dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let dialog_options = folder_dialog.GetOptions()?;
        folder_dialog.SetOptions(dialog_options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)?;
        match folder_dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(show_error) if show_error.code() == ERROR_CANCELLED.to_hresult() => return Ok(None),
            Err(show_error) => return Err(show_error),
        }
        let selected_folder = folder_dialog.GetResult()?;
        let folder_path_pointer = selected_folder.GetDisplayName(SIGDN_FILESYSPATH)?;
        let folder_path = PathBuf::from(folder_path_pointer.to_string()?);
        CoTaskMemFree(Some(folder_path_pointer.0 as *const c_void));
        Ok(Some(folder_path))
    }
}
