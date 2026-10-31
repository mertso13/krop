use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::core::Result;

pub struct ComInitialization {
    uninitialize_on_drop: bool,
}

pub fn initialize_apartment_threaded() -> Result<ComInitialization> {
    let initialization_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialization_result == RPC_E_CHANGED_MODE {
        return Ok(ComInitialization {
            uninitialize_on_drop: false,
        });
    }
    if initialization_result != S_FALSE {
        initialization_result.ok()?;
    }
    Ok(ComInitialization {
        uninitialize_on_drop: true,
    })
}

impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.uninitialize_on_drop {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
