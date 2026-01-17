/// Check if a process with the given PID is still running.
///
/// On Unix, uses `kill(pid, 0)` which checks if the process exists
/// without sending a signal.
///
/// On Windows, uses `OpenProcess` with minimal access to check existence.
pub fn is_process_alive(pid: i64) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        match kill(Pid::from_raw(pid as i32), None) {
            Ok(_) => true,                            // Process exists
            Err(nix::errno::Errno::ESRCH) => false,   // No such process
            Err(_) => true,                           // Other error (e.g., permission) - assume alive
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
            if handle.is_null() {
                false
            } else {
                CloseHandle(handle);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_process_is_alive() {
        // Current process should always be alive
        let pid = std::process::id() as i64;
        assert!(is_process_alive(pid), "Current process should be alive");
    }

    #[test]
    fn test_invalid_pid_is_not_alive() {
        // PID 0 is special (kernel) and shouldn't be "alive" in our sense
        // PID -1 is invalid
        // A very high PID is unlikely to exist
        assert!(!is_process_alive(999999999), "Non-existent PID should not be alive");
    }
}
