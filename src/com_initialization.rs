use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::core::Result;

pub struct ComInitialization(());

pub fn initialize_apartment_threaded() -> Result<ComInitialization> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
    Ok(ComInitialization(()))
}

impl Drop for ComInitialization {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
