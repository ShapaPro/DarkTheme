use crate::hook::{install_hook_for_thread, recolor_pass, set_active_colors, uninstall_all_hooks};
use dark_inject_common::config::Config;
use dark_inject_common::enum_windows::{get_class_name, windows_of_process};
use dark_inject_common::shared_state::SharedFlag;
use dark_inject_common::win32::*;
use std::io::Write;
use std::path::PathBuf;

const STATE_NAME: &str = "Local\\DarkInject1C_State";
const TH32CS_SNAPTHREAD_LOCAL: u32 = TH32CS_SNAPTHREAD;

fn log_path(pid: u32) -> PathBuf {
    let dir = std::env::temp_dir().join("DarkInject1C");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.log", pid))
}

fn config_path(hinst: HINSTANCE) -> PathBuf {
    // dark_inject.toml лежит рядом с самой dark_inject_dll.dll. Once injected
    // into a host process, std::env::current_exe() would resolve to the
    // HOST's exe (e.g. 1cv8.exe), not this DLL — so we resolve our own path
    // via GetModuleFileNameW on the module handle DllMain received, which is
    // this DLL's own hinstance, not the host's.
    use std::os::windows::ffi::OsStringExt;
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(hinst, buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 {
        return PathBuf::from("dark_inject.toml");
    }
    let mut path = PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize]));
    path.pop();
    path.push("dark_inject.toml");
    path
}

fn load_config(hinst: HINSTANCE) -> Config {
    let path = config_path(hinst);
    Config::load_from_path(&path).unwrap_or_else(|_| Config::default_colors())
}

fn threads_of_current_process() -> Vec<u32> {
    let mut result = Vec::new();
    let pid = unsafe { GetCurrentProcessId() };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD_LOCAL, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return result;
        }
        let mut entry = THREADENTRY32::default();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        if Thread32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32OwnerProcessID == pid {
                    result.push(entry.th32ThreadID);
                }
                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    result
}

fn log_current_window_hierarchy(pid: u32) {
    let path = log_path(pid);
    let mut f = match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = writeln!(f, "--- window hierarchy at injection, pid={} ---", pid);
    for hwnd in windows_of_process(pid) {
        let class_name = get_class_name(hwnd);
        let _ = writeln!(f, "hwnd={:#x} class={}", hwnd, class_name);
    }
}

pub extern "system" fn run(param: *mut std::ffi::c_void) -> u32 {
    let pid = unsafe { GetCurrentProcessId() };
    let hinst = param as HINSTANCE;

    let config = load_config(hinst);
    set_active_colors(config.colors);

    log_current_window_hierarchy(pid);

    for thread_id in threads_of_current_process() {
        install_hook_for_thread(thread_id);
    }

    let flag = SharedFlag::open_or_create(STATE_NAME).ok();
    // По умолчанию (свежий mapping) флаг = false; включаем сразу, если это
    // первый процесс, поднявший данный mapping.
    if let Some(ref f) = flag {
        f.set(true);
    }

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let enabled = flag.as_ref().map(|f| f.get()).unwrap_or(true);
        if enabled {
            recolor_pass(pid);
        }
    }
}

pub fn shutdown() {
    uninstall_all_hooks();
}
