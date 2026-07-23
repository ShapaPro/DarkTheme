mod scanner;
mod inject;
mod tray;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

const TARGET_PROCESS_NAME: &str = "1cv8.exe";
const POLL_INTERVAL_MS: u64 = 200;

fn dll_path() -> PathBuf {
    let mut exe_dir = std::env::current_exe().unwrap_or_default();
    exe_dir.pop();
    exe_dir.join("dark_inject_dll.dll")
}

fn main() {
    let dll = dll_path();
    if !dll.exists() {
        eprintln!("dark_inject_dll.dll not found at {:?} — place it next to watcher.exe", dll);
        return;
    }

    let hwnd = tray::create_tray_window();

    let mut injected: HashSet<u32> = HashSet::new();

    // Начальная выборка — инжектируем в уже запущенные 1cv8.exe.
    for pid in scanner::find_processes_by_name(TARGET_PROCESS_NAME) {
        if inject::inject_dll(pid, &dll).is_ok() {
            injected.insert(pid);
        }
    }

    loop {
        tray::pump_messages_nonblocking(hwnd);

        for pid in scanner::find_processes_by_name(TARGET_PROCESS_NAME) {
            if !injected.contains(&pid) {
                match inject::inject_dll(pid, &dll) {
                    Ok(()) => {
                        injected.insert(pid);
                    }
                    Err(e) => {
                        eprintln!("injection into pid {} failed: {}", pid, e);
                        injected.insert(pid); // не ретраим бесконечно тот же PID
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
}
