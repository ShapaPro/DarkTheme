// 1С не использует ни одного стандартного Windows-контрола для содержимого
// (см. CLAUDE.md "Главная находка") — весь UI, включая дерево метаданных,
// рисуется самой 1С через cairo (см. cairohook.rs — это основной, реально
// эффективный механизм). GDI-хуки здесь оставлены как вторичный, подстраховочный
// слой: часть более простых элементов (некоторые системные диалоги,
// стандартные Windows-контролы, если где-то всё же встретятся) может
// по-прежнему использовать классический GDI напрямую.
//
// Механизм: патчим IAT (Import Address Table) — не главного модуля
// конкретно, а ВСЕХ загруженных модулей процесса — для нескольких функций
// gdi32.dll (SetBkColor, CreateSolidBrush, SetTextColor), перенаправляя
// вызовы 1С через наши обёртки. Это не глобальный хук на весь рабочий стол:
// патчится только IAT уже открытых модулей ОДНОГО уже инжектированного
// процесса (мы и так уже внутри него).

use dark_inject_common::config::Colors;
use dark_inject_common::iat::{for_each_import_across_process, patch_iat_slot};
use dark_inject_common::win32::*;
use std::collections::HashSet;
use std::sync::Mutex;

static ACTIVE_COLORS: Mutex<Colors> = Mutex::new(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
// См. cairohook.rs::PATCHED_SLOTS за тем, почему это нужно: 1С подгружает
// часть модулей лениво, повторный скан не должен перепатчивать уже
// подмененные слоты (иначе наш собственный hook попадёт в REAL_* и вызов
// уйдёт в бесконечную рекурсию).
static PATCHED_SLOTS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

pub fn set_active_colors(colors: Colors) {
    *ACTIVE_COLORS.lock().unwrap() = colors;
}

type SetBkColorFn = extern "system" fn(HDC, u32) -> u32;
type SetTextColorFn = extern "system" fn(HDC, u32) -> u32;
type CreateSolidBrushFn = extern "system" fn(u32) -> HBRUSH;

static mut REAL_SET_BK_COLOR: Option<SetBkColorFn> = None;
static mut REAL_SET_TEXT_COLOR: Option<SetTextColorFn> = None;
static mut REAL_CREATE_SOLID_BRUSH: Option<CreateSolidBrushFn> = None;

fn channels(color: u32) -> (u32, u32, u32) {
    // COLORREF = 0x00BBGGRR
    (color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF)
}

/// Светлый нейтральный цвет (типичный фон: белый/бежевый/светло-серый) —
/// все каналы достаточно яркие. Насыщенные акцентные цвета (выделение,
/// иконки) обычно имеют хотя бы один тёмный канал и сюда не попадают.
fn is_light_background(color: u32) -> bool {
    let (r, g, b) = channels(color);
    r.min(g).min(b) > 140 && (r + g + b) > 3 * 170
}

/// Тёмный нейтральный цвет (типичный текст: чёрный/тёмно-серый). Требуем ещё
/// и низкую насыщенность (max-min каналов мал) — иначе насыщенный акцент,
/// у которого один канал случайно тёмный (например чистый синий 0x00FF0000
/// в BBGGRR: R=0,G=0,B=255), ошибочно попадёт под "это текст, надо
/// осветлить" только из-за низкой суммы каналов.
fn is_dark_foreground(color: u32) -> bool {
    let (r, g, b) = channels(color);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    (r + g + b) < 3 * 90 && (max - min) < 40
}

extern "system" fn hook_set_bk_color(hdc: HDC, color: u32) -> u32 {
    let mapped = if is_light_background(color) {
        ACTIVE_COLORS.lock().unwrap().bg
    } else {
        color
    };
    unsafe { REAL_SET_BK_COLOR.expect("hook installed without real fn")(hdc, mapped) }
}

extern "system" fn hook_set_text_color(hdc: HDC, color: u32) -> u32 {
    let mapped = if is_dark_foreground(color) {
        ACTIVE_COLORS.lock().unwrap().text
    } else {
        color
    };
    unsafe { REAL_SET_TEXT_COLOR.expect("hook installed without real fn")(hdc, mapped) }
}

extern "system" fn hook_create_solid_brush(color: u32) -> HBRUSH {
    let mapped = if is_light_background(color) {
        ACTIVE_COLORS.lock().unwrap().bg
    } else {
        color
    };
    unsafe { REAL_CREATE_SOLID_BRUSH.expect("hook installed without real fn")(mapped) }
}

/// Патчит IAT всех модулей процесса, подменяя SetBkColor/SetTextColor/
/// CreateSolidBrush из gdi32.dll на наши обёртки. Идемпотентно (уже
/// пропатченные слоты пропускаются, см. PATCHED_SLOTS) — безопасно вызывать
/// многократно, чтобы поймать модули, догруженные уже после инъекции.
pub fn rescan() {
    let mut patched = PATCHED_SLOTS.lock().unwrap();
    let seen = patched.get_or_insert_with(HashSet::new);
    unsafe {
        for_each_import_across_process(
            "gdi32.dll",
            &["SetBkColor", "SetTextColor", "CreateSolidBrush"],
            |name, slot| {
                let key = slot as usize;
                if !seen.insert(key) {
                    return;
                }
                match name {
                    "SetBkColor" => {
                        let original = patch_iat_slot(slot, hook_set_bk_color as *const () as usize);
                        REAL_SET_BK_COLOR = Some(std::mem::transmute::<usize, SetBkColorFn>(original));
                    }
                    "SetTextColor" => {
                        let original = patch_iat_slot(slot, hook_set_text_color as *const () as usize);
                        REAL_SET_TEXT_COLOR = Some(std::mem::transmute::<usize, SetTextColorFn>(original));
                    }
                    "CreateSolidBrush" => {
                        let original = patch_iat_slot(slot, hook_create_solid_brush as *const () as usize);
                        REAL_CREATE_SOLID_BRUSH = Some(std::mem::transmute::<usize, CreateSolidBrushFn>(original));
                    }
                    _ => {}
                }
            },
        );
    }
}

pub fn install() {
    rescan();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_light_background_colors() {
        assert!(is_light_background(0x00FFFFFF)); // белый
        assert!(is_light_background(0x00C8D0D4)); // типичный бежевый Windows 3D-face (BBGGRR)
        assert!(!is_light_background(0x00000000)); // чёрный
        assert!(!is_light_background(0x00FF0000)); // насыщенный синий (BBGGRR: BB=FF) — акцент, не трогаем
    }

    #[test]
    fn classifies_dark_foreground_colors() {
        assert!(is_dark_foreground(0x00000000)); // чёрный текст
        assert!(!is_dark_foreground(0x00FFFFFF)); // белый — уже светлый, не текст на светлом фоне
        assert!(!is_dark_foreground(0x00FF0000)); // насыщенный акцент — не обычный текст
    }

    #[test]
    fn finds_real_gdi32_imports_in_this_process() {
        // Простое extern-объявление НЕ создаёт импорт в IAT — линкер включает
        // символ, только если его кто-то реально вызывает. Форсируем настоящий
        // вызов SetBkColor, чтобы у ЭТОГО тестового экзешника точно появился
        // настоящий IAT-импорт gdi32.dll, и проверяем сканер across-process
        // на нём без инъекции в 1С.
        unsafe {
            let hdc = GetDC(0);
            SetBkColor(hdc, 0x00FFFFFF);
            ReleaseDC(0, hdc);
        }

        let mut found = Vec::new();
        unsafe {
            for_each_import_across_process("gdi32.dll", &["SetBkColor", "SetTextColor", "CreateSolidBrush"], |name, slot| {
                found.push((name.to_string(), slot as usize));
            });
        }
        assert!(
            !found.is_empty(),
            "expected to find at least one gdi32 import (SetBkColor/SetTextColor/CreateSolidBrush) across this test process's modules"
        );
    }
}
