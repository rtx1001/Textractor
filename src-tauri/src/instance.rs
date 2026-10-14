use windows::{core::PCWSTR, Win32::{Foundation::{CloseHandle, GetLastError, HANDLE, ERROR_ALREADY_EXISTS}, System::Threading::CreateMutexW}};

// Keep a named kernel handle for the lifetime of the application. Windows
// releases it on exit/crash, unlike a lock file that can leave stale state.
pub struct InstanceGuard(HANDLE);
impl Drop for InstanceGuard {
    fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } }
}

pub fn acquire(name: &str) -> Result<Option<InstanceGuard>, String> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr())).map_err(|e| e.to_string())?;
        // Check immediately: other Win32 calls can overwrite the last error.
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            Ok(None)
        } else { Ok(Some(InstanceGuard(handle))) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_is_rejected_and_exit_releases_instance() {
        let name = format!("Local\\Textractor.Instance.Test.{}", std::process::id());
        let first = acquire(&name).unwrap().expect("first launch");
        assert!(acquire(&name).unwrap().is_none(), "duplicate must not start or register hotkeys");
        drop(first);
        assert!(acquire(&name).unwrap().is_some(), "restart after exit must work");
    }
}
