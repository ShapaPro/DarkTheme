use dark_inject_common::win32::*;

pub fn find_processes_by_name(name: &str) -> Vec<u32> {
    let mut result = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return result;
        }
        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let exe_name = wide_to_string(&entry.szExeFile);
                if exe_name.eq_ignore_ascii_case(name) {
                    result.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    result
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_current_process_by_its_own_exe_name() {
        let current_exe = std::env::current_exe().expect("current_exe");
        let exe_name = current_exe
            .file_name()
            .expect("file_name")
            .to_str()
            .expect("utf8");

        let pids = find_processes_by_name(exe_name);
        let my_pid = std::process::id();

        assert!(
            pids.contains(&my_pid),
            "expected {:?} to contain current pid {}",
            pids,
            my_pid
        );
    }

    #[test]
    fn returns_empty_for_nonexistent_name() {
        let pids = find_processes_by_name("definitely_not_a_real_process_xyz123.exe");
        assert!(pids.is_empty());
    }
}
