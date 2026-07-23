use crate::color::color_window;
use dark_inject_common::classify::classify_window;
use dark_inject_common::config::Colors;
use dark_inject_common::enum_windows::{get_class_name, windows_of_process};
use dark_inject_common::win32::*;
use std::sync::Mutex;

static ACTIVE_COLORS: Mutex<Colors> = Mutex::new(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
static INSTALLED_HOOKS: Mutex<Vec<HHOOK>> = Mutex::new(Vec::new());

pub fn set_active_colors(colors: Colors) {
    *ACTIVE_COLORS.lock().unwrap() = colors;
}

extern "system" fn cbt_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HCBT_CREATEWND {
        let hwnd = wparam as HWND;
        // At HCBT_CREATEWND the HWND already exists (and its class name is
        // already resolvable) but the control has not processed WM_CREATE
        // yet. This is early enough to set the DWM immersive-dark-mode
        // attribute and the SetWindowTheme association (both are simple
        // window-level properties, not dependent on the control's internal
        // state), but TOO EARLY for TVM_*/LVM_* messages on real common
        // controls — see the WH_CALLWNDPROCRET hook below, which is what
        // actually makes those stick.
        on_window_created(hwnd);
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

/// Fires right after the target window's own WM_CREATE handling has run
/// (nCode == HC_ACTION means "safe to process"). By this point real common
/// controls (confirmed empirically for SysTreeView32 in Task 8) have
/// allocated the internal state that TVM_*/LVM_* color messages write into,
/// so the colors actually take effect — sending them any earlier (at
/// HCBT_CREATEWND, or even right after WM_NCCREATE) is a silent no-op. This
/// is still entirely synchronous and still strictly before first paint — no
/// message loop has had a chance to run yet, all of this happens inside the
/// original CreateWindowEx call before it returns to its caller.
extern "system" fn callwndprocret_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HC_ACTION {
        let cwp = lparam as *const CWPRETSTRUCT;
        if !cwp.is_null() {
            let msg = unsafe { (*cwp).message };
            if msg == WM_CREATE {
                let hwnd = unsafe { (*cwp).hwnd };
                on_window_created(hwnd);
            }
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

fn on_window_created(hwnd: HWND) {
    let class_name = get_class_name(hwnd);
    let kind = classify_window(&class_name);
    let colors = *ACTIVE_COLORS.lock().unwrap();
    color_window(hwnd, kind, &colors);
}

/// Ставит ЛОКАЛЬНЫЕ, потоковые хуки (dwThreadId != 0). НИКОГДА не вызывать с
/// thread_id = 0 — это сделало бы хук глобальным для всего рабочего стола.
///
/// Ставятся два хука на один и тот же поток:
/// - WH_CBT / HCBT_CREATEWND — раскрашивает DWM-рамку и назначает
///   SetWindowTheme как можно раньше (сразу после выделения HWND).
/// - WH_CALLWNDPROCRET, реагирующий на WM_CREATE — докрашивает TVM_*/LVM_*
///   уже после того, как контрол выделил своё внутреннее состояние для
///   хранения цвета (проверено эмпирически на реальном SysTreeView32:
///   TVM_SETBKCOLOR в момент HCBT_CREATEWND и даже сразу после WM_NCCREATE —
///   молчаливый no-op, состояние выделяется только в WM_CREATE). Оба хука
///   синхронны и отрабатывают строго до первой отрисовки — цикл сообщений
///   ещё не запущен на этот момент.
///
/// Возвращает HHOOK хука WH_CBT (второй хук хранится и снимается вместе с
/// ним через uninstall_all_hooks, но по сигнатуре наружу не отдаётся).
pub fn install_hook_for_thread(thread_id: u32) -> HHOOK {
    unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_CBT, cbt_hook_proc, hmod, thread_id);
        if hook != 0 {
            INSTALLED_HOOKS.lock().unwrap().push(hook);
        }
        let post_create_hook =
            SetWindowsHookExW(WH_CALLWNDPROCRET, callwndprocret_hook_proc, hmod, thread_id);
        if post_create_hook != 0 {
            INSTALLED_HOOKS.lock().unwrap().push(post_create_hook);
        }
        hook
    }
}

pub fn uninstall_all_hooks() {
    let mut hooks = INSTALLED_HOOKS.lock().unwrap();
    for hook in hooks.drain(..) {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }
}

/// Fallback: докрашивает окна процесса pid, которые могли быть созданы на
/// потоках, не покрытых install_hook_for_thread (например, поток появился
/// после инъекции).
pub fn recolor_pass(pid: u32) {
    let colors = *ACTIVE_COLORS.lock().unwrap();
    for hwnd in windows_of_process(pid) {
        let class_name = get_class_name(hwnd);
        let kind = classify_window(&class_name);
        color_window(hwnd, kind, &colors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dark_inject_common::config::Colors;

    #[test]
    fn cbt_hook_precolors_window_created_on_same_thread() {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES,
        };
        unsafe { InitCommonControlsEx(&icc) };

        let colors = Colors { bg: 0xABCDEF, text: 0x111111, line: 0x222222 };
        set_active_colors(colors);

        let thread_id = unsafe { GetCurrentThreadId() };
        let hook = install_hook_for_thread(thread_id);
        assert_ne!(hook, 0, "SetWindowsHookExW failed");

        // Потоковый (не глобальный) хук вызывается синхронно в момент
        // CreateWindow на этом же потоке — сообщение НЕ требуется.
        let class_wide = to_wide("SysTreeView32");
        let name_wide = to_wide("");
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_wide.as_ptr(),
                name_wide.as_ptr(),
                WS_CHILD,
                0,
                0,
                100,
                100,
                HWND_MESSAGE,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(hwnd, 0);

        // Если хук уже покрасил окно в colors.bg, второй SET вернёт colors.bg
        // как "предыдущее" значение.
        let other_bg: u32 = 0x999999;
        let prev = unsafe { SendMessageW(hwnd, TVM_SETBKCOLOR, 0, other_bg as isize) };
        assert_eq!(prev as u32, colors.bg, "window was not pre-colored by CBT hook before first paint");

        unsafe { DestroyWindow(hwnd) };
        uninstall_all_hooks();
    }
}
