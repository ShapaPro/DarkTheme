use dark_inject_common::classify::WindowKind;
use dark_inject_common::config::Colors;
use dark_inject_common::win32::*;

pub fn color_window(hwnd: HWND, kind: WindowKind, colors: &Colors) {
    unsafe {
        // Тёмная рамка/заголовок — только для top-level окон (нет WS_CHILD).
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if style & WS_CHILD == 0 {
            let val: i32 = 1;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &val as *const i32 as *const _,
                std::mem::size_of::<i32>() as u32,
            );
        }

        match kind {
            WindowKind::TreeView => {
                SendMessageW(hwnd, TVM_SETBKCOLOR, 0, colors.bg as isize);
                SendMessageW(hwnd, TVM_SETTEXTCOLOR, 0, colors.text as isize);
                SendMessageW(hwnd, TVM_SETLINECOLOR, 0, colors.line as isize);
                let theme = to_wide("DarkMode_Explorer");
                SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
            }
            WindowKind::ListView => {
                SendMessageW(hwnd, LVM_SETBKCOLOR, 0, colors.bg as isize);
                SendMessageW(hwnd, LVM_SETTEXTCOLOR, 0, colors.text as isize);
                SendMessageW(hwnd, LVM_SETTEXTBKCOLOR, 0, colors.bg as isize);
                let theme = to_wide("DarkMode_Explorer");
                SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
            }
            WindowKind::Header => {
                // HDM_SETBKCOLOR сознательно НЕ отправляется: это не реальное
                // Win32-сообщение для Header control — 0x1201 совпадает с
                // HDM_INSERTITEMA, которое трактует lParam как указатель на
                // HDITEMA. Отправка цвета в lParam вызывает разыменование
                // мусорного указателя и STATUS_ACCESS_VIOLATION (проверено
                // эмпирически на реальном SysHeader32 в тестах этого модуля).
                // Красим заголовки колонок только через SetWindowTheme.
                let theme = to_wide("DarkMode_ItemsView");
                SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
            }
            WindowKind::Other => {
                // Скроллбары и прочее — тот же приём, без гарантии покрытия.
                let theme = to_wide("DarkMode_Explorer");
                SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_common_controls() {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES | ICC_LISTVIEW_CLASSES,
        };
        unsafe {
            InitCommonControlsEx(&icc);
        }
    }

    fn create_hidden_control(class_name: &str) -> HWND {
        init_common_controls();
        let class_wide = to_wide(class_name);
        let name_wide = to_wide("");
        unsafe {
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
        }
    }

    #[test]
    fn colors_treeview_bg_via_set_message_return_value() {
        let hwnd = create_hidden_control("SysTreeView32");
        assert_ne!(hwnd, 0, "failed to create SysTreeView32 test window");

        let colors_a = Colors { bg: 0x111111, text: 0x222222, line: 0x333333 };
        let colors_b = Colors { bg: 0x444444, text: 0x555555, line: 0x666666 };

        color_window(hwnd, WindowKind::TreeView, &colors_a);
        // TVM_SETBKCOLOR возвращает ПРЕДЫДУЩИЙ цвет — вызываем второй раз и
        // проверяем, что предыдущим было именно colors_a.bg.
        let prev = unsafe { SendMessageW(hwnd, TVM_SETBKCOLOR, 0, colors_b.bg as isize) };
        assert_eq!(prev as u32, colors_a.bg);

        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn colors_listview_bg_via_set_message_return_value() {
        let hwnd = create_hidden_control("SysListView32");
        assert_ne!(hwnd, 0, "failed to create SysListView32 test window");

        let colors_a = Colors { bg: 0x111111, text: 0x222222, line: 0x333333 };

        color_window(hwnd, WindowKind::ListView, &colors_a);
        // ВАЖНО (найдено эмпирически в Task 7): в отличие от TVM_SETBKCOLOR,
        // реальный LVM_SETBKCOLOR возвращает BOOL (успех/неудача), а НЕ
        // предыдущий цвет — round-trip через return value второго вызова
        // здесь не работает и всегда даёт 1 (TRUE), а не старый цвет.
        // Поэтому колор проверяем через LVM_GETBKCOLOR — это корректно
        // документированный способ прочитать текущий фон ListView.
        let current = unsafe { SendMessageW(hwnd, LVM_GETBKCOLOR, 0, 0) };
        assert_eq!(current as u32, colors_a.bg);

        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn header_coloring_does_not_panic_smoke_test() {
        // ICC_LISTVIEW_CLASSES регистрирует и SysHeader32.
        let hwnd = create_hidden_control("SysHeader32");
        assert_ne!(hwnd, 0, "failed to create SysHeader32 test window");

        let colors = Colors { bg: 0x111111, text: 0x222222, line: 0x333333 };
        color_window(hwnd, WindowKind::Header, &colors); // не должно паниковать

        unsafe { DestroyWindow(hwnd) };
    }
}
