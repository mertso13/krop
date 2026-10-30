use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::core::Result;

pub fn enable_per_monitor_v2_awareness() -> Result<()> {
    match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
        Err(error) if error.code() == ERROR_ACCESS_DENIED.to_hresult() => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_PER_MONITOR_AWARE, GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext,
    };

    const CHILD_PROCESS_VARIABLE: &str = "KROP_DPI_AWARENESS_TEST_CHILD";
    const AWARENESS_TEST_NAME: &str = "dpi_awareness::tests::per_monitor_v2_awareness_is_enabled";

    fn current_awareness() -> windows::Win32::UI::HiDpi::DPI_AWARENESS {
        unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) }
    }

    #[test]
    fn per_monitor_v2_awareness_is_enabled() {
        if std::env::var_os(CHILD_PROCESS_VARIABLE).is_some() {
            enable_per_monitor_v2_awareness().unwrap();
            assert_eq!(current_awareness(), DPI_AWARENESS_PER_MONITOR_AWARE);
            return;
        }

        let child_output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", AWARENESS_TEST_NAME, "--nocapture"])
            .env(CHILD_PROCESS_VARIABLE, "1")
            .output()
            .unwrap();
        let child_stdout = String::from_utf8_lossy(&child_output.stdout);

        assert!(child_output.status.success(), "{child_stdout}");
        assert!(child_stdout.contains("1 passed"), "{child_stdout}");
    }
}
