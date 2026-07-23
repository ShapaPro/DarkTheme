use crate::win32::*;
use std::sync::Mutex;

static COLLECTOR: Mutex<Vec<HWND>> = Mutex::new(Vec::new());
static TARGET_PID: Mutex<u32> = Mutex::new(0);

extern "system" fn enum_proc(hwnd: HWND, _lparam: isize) -> i32 {
    let target = *TARGET_PID.lock().unwrap();
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == target {
        COLLECTOR.lock().unwrap().push(hwnd);
    }
    1
}

/// Возвращает top-level окна, принадлежащие процессу с данным pid.
/// НЕ трогает окна других процессов — используется fallback-перекраской и
/// логгером иерархии внутри инжектированной DLL, где pid всегда равен
/// GetCurrentProcessId().
pub fn windows_of_process(pid: u32) -> Vec<HWND> {
    *TARGET_PID.lock().unwrap() = pid;
    COLLECTOR.lock().unwrap().clear();
    unsafe {
        EnumWindows(enum_proc, 0);
    }
    COLLECTOR.lock().unwrap().clone()
}

pub fn get_class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_window_created_in_current_process() {
        // NB: this must be a real top-level window (parent = NULL, no
        // WS_CHILD), NOT a HWND_MESSAGE-parented message-only window.
        // Message-only windows are documented (and confirmed empirically
        // here) to be invisible to EnumWindows — they aren't top-level,
        // visible, or owned windows, so windows_of_process (built on
        // EnumWindows) can never find one. Using HWND_MESSAGE here would
        // make this test unable to pass regardless of whether
        // windows_of_process itself is correct.
        let class_wide = to_wide("STATIC");
        let name_wide = to_wide("");
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_wide.as_ptr(),
                name_wide.as_ptr(),
                0,
                0,
                0,
                10,
                10,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(hwnd, 0);

        let pid = unsafe { GetCurrentProcessId() };
        let found = windows_of_process(pid);
        assert!(found.contains(&hwnd));

        unsafe { DestroyWindow(hwnd) };
    }
}
