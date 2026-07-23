use crate::hook::{install_hook_for_thread, recolor_pass, set_active_colors, uninstall_all_hooks};
use dark_inject_common::config::Config;
use dark_inject_common::enum_windows::{get_class_name, get_window_rect, get_window_text, windows_of_process};
use dark_inject_common::shared_state::SharedFlag;
use dark_inject_common::win32::*;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
struct WindowInfo {
    hwnd: HWND,
    class: String,
    parent: HWND,
    rect: Option<RECT>,
    text: String,
}

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

fn current_snapshot(pid: u32) -> Vec<WindowInfo> {
    windows_of_process(pid)
        .into_iter()
        .map(|hwnd| WindowInfo {
            hwnd,
            class: get_class_name(hwnd),
            parent: unsafe { GetParent(hwnd) },
            rect: get_window_rect(hwnd),
            text: get_window_text(hwnd),
        })
        .collect()
}

/// Дописывает снимок иерархии в лог, только если он отличается от
/// предыдущего. Инъекция обычно происходит раньше, чем 1С успевает создать
/// реальные окна (метаданные появляются только после того, как пользователь
/// вручную откроет базу и конфигурацию — это может занять произвольное время,
/// вплоть до нескольких минут). Разовый снимок при инъекции или снимок с
/// ограничением по количеству тиков (что пробовалось раньше) может не
/// дождаться этого момента. Сравнение с предыдущим снимком даёт лог без
/// ограничения по времени и без неограниченного роста на пустом месте:
/// новая запись появляется только когда реально что-то изменилось.
///
/// Помимо класса, пишем родителя/размер/текст окна — простой class name
/// оказался недостаточным диагностическим сигналом (1С использует общий
/// класс "V8Window" для очень разных по смыслу панелей), а координаты и
/// родитель позволяют вручную сопоставить конкретный hwnd с тем, что видно
/// на экране.
fn log_hierarchy_if_changed(pid: u32, last: &mut Option<Vec<WindowInfo>>) {
    let snapshot = current_snapshot(pid);
    if last.as_ref() == Some(&snapshot) {
        return;
    }
    let path = log_path(pid);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "--- window hierarchy changed, pid={} ---", pid);
        for w in &snapshot {
            let rect_str = match w.rect {
                Some(r) => format!("{}x{}", r.right - r.left, r.bottom - r.top),
                None => "?".to_string(),
            };
            let _ = writeln!(
                f,
                "hwnd={:#x} parent={:#x} class={} size={} text=\"{}\"",
                w.hwnd, w.parent, w.class, rect_str, w.text
            );
        }
    }
    *last = Some(snapshot);
}

pub extern "system" fn run(param: *mut std::ffi::c_void) -> u32 {
    let pid = unsafe { GetCurrentProcessId() };
    let hinst = param as HINSTANCE;

    let config = load_config(hinst);
    set_active_colors(config.colors);
    crate::gdihook::set_active_colors(config.colors);
    crate::cairohook::set_active_colors(config.colors);
    // Красим содержимое (не только рамку) через подмену GDI- и cairo-вызовов
    // в IAT всех модулей процесса — см. gdihook.rs/cairohook.rs за тем, почему
    // обычный SendMessage-подход (color.rs/hook.rs) не имеет адресата в этом
    // конкретном приложении (1С рисует всё сама, реальный рендер — cairo).
    crate::gdihook::install();
    crate::cairohook::install();

    let mut last_hierarchy: Option<Vec<WindowInfo>> = None;
    log_hierarchy_if_changed(pid, &mut last_hierarchy);

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
        log_hierarchy_if_changed(pid, &mut last_hierarchy);
    }
}

pub fn shutdown() {
    uninstall_all_hooks();
}
