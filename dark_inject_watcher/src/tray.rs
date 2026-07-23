use dark_inject_common::shared_state::SharedFlag;
use dark_inject_common::win32::*;
use std::sync::OnceLock;

const STATE_NAME: &str = "Local\\DarkInject1C_State";
const HOTKEY_ID: i32 = 1;
static FLAG: OnceLock<SharedFlag> = OnceLock::new();

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_HOTKEY && wparam as i32 == HOTKEY_ID {
        if let Some(flag) = FLAG.get() {
            let new_value = !flag.get();
            flag.set(new_value);
        }
        return 0;
    }
    if msg == WM_DESTROY {
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Создаёт скрытое окно-обработчик сообщений для трея и глобального хоткея.
/// Возвращает hwnd или 0 при ошибке.
pub fn create_tray_window() -> HWND {
    let _ = FLAG.set(SharedFlag::open_or_create(STATE_NAME).expect("shared flag"));
    if let Some(f) = FLAG.get() {
        f.set(true); // включено по умолчанию при старте watcher'а
    }

    let class_name = to_wide("DarkInject1CTrayWindow");
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wndclass = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wnd_proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: 0,
        hCursor: 0,
        hbrBackground: 0,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    unsafe {
        RegisterClassW(&wndclass);
        let name_wide = to_wide("DarkInject1C");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            name_wide.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            hinstance,
            std::ptr::null_mut(),
        );
        if hwnd != 0 {
            RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_ALT, VK_D);
        }
        hwnd
    }
}

/// Обрабатывает все накопившиеся сообщения без блокировки — вызывается из
/// главного цикла watcher'а между итерациями поллинга процессов.
pub fn pump_messages_nonblocking(hwnd: HWND) {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
