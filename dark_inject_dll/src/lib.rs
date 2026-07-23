pub mod color;
pub mod hook;
pub mod worker;

use std::ffi::c_void;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(hinst: isize, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        r if r == DLL_PROCESS_ATTACH => unsafe {
            let thread = dark_inject_common::win32::CreateThread(
                std::ptr::null_mut(),
                0,
                worker::run,
                hinst as *mut c_void,
                0,
                std::ptr::null_mut(),
            );
            if thread != 0 {
                dark_inject_common::win32::CloseHandle(thread);
            }
        },
        r if r == DLL_PROCESS_DETACH => {
            worker::shutdown();
        }
        _ => {}
    }
    1
}
