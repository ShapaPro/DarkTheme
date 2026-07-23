use std::path::PathBuf;
use std::time::{Duration, Instant};

extern "system" {
    fn LoadLibraryW(name: *const u16) -> isize;
    fn FreeLibrary(hmodule: isize) -> i32;
    fn GetCurrentProcessId() -> u32;
}

fn dll_path() -> PathBuf {
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join(profile)
        .join("dark_inject_dll.dll")
}

fn to_wide(s: &str) -> Vec<u16> {
    use std::iter::once;
    s.encode_utf16().chain(once(0)).collect()
}

#[test]
fn dllmain_logs_window_hierarchy_of_its_own_process() {
    let path = dll_path();
    assert!(path.exists(), "dark_inject_dll.dll not built at {:?}", path);

    let pid = unsafe { GetCurrentProcessId() };
    let log_path = std::env::temp_dir().join("DarkInject1C").join(format!("{}.log", pid));
    let _ = std::fs::remove_file(&log_path);

    let wide = to_wide(path.to_str().unwrap());
    let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
    assert_ne!(handle, 0, "LoadLibraryW failed for dark_inject_dll.dll");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(&log_path) {
            if contents.contains("window hierarchy at injection") {
                found = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(found, "expected log file {:?} to contain window hierarchy dump", log_path);

    unsafe { FreeLibrary(handle) };
    let _ = std::fs::remove_file(&log_path);
}
