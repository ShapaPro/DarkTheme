use crate::win32::*;

// Контекст передаётся через lParam (указатель на стековую структуру), а не
// через глобальные static — enum_windows может вызываться из нескольких
// потоков одновременно (например, параллельные тесты), и глобальные
// Mutex<Vec>/Mutex<u32>, обновляемые отдельными lock()-ами, не образуют
// одной атомарной секции: один поток мог перезаписать TARGET_PID/COLLECTOR
// между вызовом EnumWindows другого потока и чтением его результата.
// Подтверждено эмпирически: `cargo test` (параллельные потоки по умолчанию)
// ловил гонку и терял ожидаемые окна. lParam-контекст полностью устраняет
// разделяемое состояние между вызовами.
struct EnumCtx {
    target_pid: u32,
    found: Vec<HWND>,
}

extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
    let ctx = unsafe { &mut *(lparam as *mut EnumCtx) };
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == ctx.target_pid {
        ctx.found.push(hwnd);
    }
    1
}

/// Возвращает top-level окна, принадлежащие процессу с данным pid, И все их
/// дочерние окна рекурсивно (дерево метаданных/списки почти всегда дочерние
/// окна какого-то top-level фрейма, а не top-level сами по себе — без этого
/// рекурсивного обхода они никогда не попадут ни в лог, ни в fallback-
/// перекраску). НЕ трогает окна других процессов — используется fallback-
/// перекраской и логгером иерархии внутри инжектированной DLL, где pid
/// всегда равен GetCurrentProcessId().
pub fn windows_of_process(pid: u32) -> Vec<HWND> {
    let mut ctx = EnumCtx { target_pid: pid, found: Vec::new() };
    unsafe {
        EnumWindows(enum_proc, &mut ctx as *mut EnumCtx as isize);
    }
    let top_level = ctx.found;

    let mut all = top_level.clone();
    for hwnd in top_level {
        let mut child_ctx = EnumCtx { target_pid: pid, found: Vec::new() };
        unsafe {
            EnumChildWindows(hwnd, enum_proc, &mut child_ctx as *mut EnumCtx as isize);
        }
        all.extend(child_ctx.found);
    }
    all
}

pub fn get_class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_window_created_in_current_process() {
        // NB: this must be a real top-level window (parent = NULL, no
        // WS_CHILD), NOT a HWND_MESSAGE-parented message-only window.
        // Message-only windows are documented (and confirmed empirically
        // here) to be invisible to EnumWindows — they aren't top-level,
        // visible, or owned windows, so windows_of_process (built on
        // EnumWindows) can never find one. Using HWND_MESSAGE here would
        // make this test unable to pass regardless of whether
        // windows_of_process itself is correct.
        let class_wide = to_wide("STATIC");
        let name_wide = to_wide("");
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_wide.as_ptr(),
                name_wide.as_ptr(),
                0,
                0,
                0,
                10,
                10,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(hwnd, 0);

        let pid = unsafe { GetCurrentProcessId() };
        let found = windows_of_process(pid);
        assert!(found.contains(&hwnd));

        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn finds_a_child_window_not_just_top_level() {
        // Реальные контролы (дерево метаделов и т.п.) почти всегда дочерние
        // окна какого-то top-level фрейма — эта регрессия (EnumWindows видит
        // только top-level) была обнаружена вживую на реальном 1cv8.exe:
        // windows_of_process возвращал только фреймы верхнего уровня,
        // ни одного дочернего контрола.
        let parent_class = to_wide("STATIC");
        let empty_name = to_wide("");
        let parent = unsafe {
            CreateWindowExW(
                0,
                parent_class.as_ptr(),
                empty_name.as_ptr(),
                0,
                0,
                0,
                50,
                50,
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(parent, 0, "failed to create top-level parent window");

        let child_class = to_wide("STATIC");
        let child = unsafe {
            CreateWindowExW(
                0,
                child_class.as_ptr(),
                empty_name.as_ptr(),
                WS_CHILD,
                0,
                0,
                10,
                10,
                parent,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(child, 0, "failed to create child window");

        let pid = unsafe { GetCurrentProcessId() };
        let found = windows_of_process(pid);

        assert!(found.contains(&parent), "expected to find the top-level parent");
        assert!(found.contains(&child), "expected to find the CHILD window too, not just top-level");

        unsafe {
            DestroyWindow(child);
            DestroyWindow(parent);
        }
    }
}
