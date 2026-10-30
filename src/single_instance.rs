use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{HSTRING, Result};

const INSTANCE_MUTEX_NAME: &str = r"Local\krop-single-instance";

pub struct SingleInstanceGuard(HANDLE);

pub fn acquire_single_instance() -> Result<Option<SingleInstanceGuard>> {
    acquire_named_instance(INSTANCE_MUTEX_NAME)
}

fn acquire_named_instance(mutex_name: &str) -> Result<Option<SingleInstanceGuard>> {
    let mutex_name = HSTRING::from(mutex_name);
    let mutex = unsafe { CreateMutexW(None, false, &mutex_name)? };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(mutex);
        }
        return Ok(None);
    }
    Ok(Some(SingleInstanceGuard(mutex)))
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_guard_can_be_held_for_a_mutex_name() {
        let mutex_name = format!(r"Local\krop-test-single-instance-{}", std::process::id());

        let first_guard = acquire_named_instance(&mutex_name).unwrap();
        assert!(first_guard.is_some());
        assert!(acquire_named_instance(&mutex_name).unwrap().is_none());

        drop(first_guard);
        assert!(acquire_named_instance(&mutex_name).unwrap().is_some());
    }
}
