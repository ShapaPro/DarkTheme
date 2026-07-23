use dark_inject_common::win32::*;
use std::path::Path;

pub fn inject_dll(pid: u32, dll_path: &Path) -> Result<(), String> {
    let dll_path_str = dll_path
        .to_str()
        .ok_or_else(|| "dll_path is not valid UTF-8".to_string())?;
    let wide_path = to_wide(dll_path_str);
    let byte_len = wide_path.len() * std::mem::size_of::<u16>();

    unsafe {
        let access = PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ;
        let process = OpenProcess(access, 0, pid);
        if process == 0 {
            return Err(format!("OpenProcess failed for pid {}", pid));
        }

        let remote_mem = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            byte_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote_mem.is_null() {
            CloseHandle(process);
            return Err("VirtualAllocEx failed".to_string());
        }

        let mut written: usize = 0;
        let ok = WriteProcessMemory(
            process,
            remote_mem,
            wide_path.as_ptr() as *const _,
            byte_len,
            &mut written,
        );
        if ok == 0 || written != byte_len {
            CloseHandle(process);
            return Err("WriteProcessMemory failed".to_string());
        }

        let kernel32 = GetModuleHandleW(to_wide("kernel32.dll").as_ptr());
        if kernel32 == 0 {
            CloseHandle(process);
            return Err("GetModuleHandleW(kernel32.dll) failed".to_string());
        }
        let load_library_w = GetProcAddress(kernel32, b"LoadLibraryW\0".as_ptr());
        if load_library_w.is_null() {
            CloseHandle(process);
            return Err("GetProcAddress(LoadLibraryW) failed".to_string());
        }

        let mut thread_id: u32 = 0;
        let thread = CreateRemoteThread(
            process,
            std::ptr::null_mut(),
            0,
            load_library_w,
            remote_mem,
            0,
            &mut thread_id,
        );
        if thread == 0 {
            CloseHandle(process);
            return Err("CreateRemoteThread failed".to_string());
        }

        WaitForSingleObject(thread, 5000);
        CloseHandle(thread);
        CloseHandle(process);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_err_for_nonexistent_pid() {
        // PID, которого почти наверняка не существует.
        let result = inject_dll(u32::MAX - 1, Path::new("C:\\does\\not\\matter.dll"));
        assert!(result.is_err());
    }
}
