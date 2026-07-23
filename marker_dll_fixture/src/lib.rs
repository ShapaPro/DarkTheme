use std::ffi::c_void;
use std::io::Write;

const DLL_PROCESS_ATTACH: u32 = 1;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_hinst: isize, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        let pid = std::process::id();
        let path = std::env::temp_dir().join("dark_inject_marker.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "loaded pid={}", pid);
        }
    }
    1
}
