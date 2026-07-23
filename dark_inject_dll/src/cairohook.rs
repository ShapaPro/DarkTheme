// Основной, реально работающий механизм перекраски содержимого. 1С не
// использует стандартные Windows-контролы (см. CLAUDE.md "Главная находка")
// и почти не красит главный UI через классический GDI (gdihook.rs — только
// подстраховка) — реальный рендеринг идёт через cairo.dll (загружается
// внутри 1cv8.exe вместе с grphcs.dll, подтверждено эмпирически через
// Get-Process -Modules на живом процессе). Единственный практичный способ
// перекрасить дерево метаданных — перехватить cairo_set_source_rgb(a),
// которыми 1С устанавливает текущий цвет перед каждой заливкой/отрисовкой
// текста, и подменять цвет на лету.
//
// В отличие от gdi32 (где SetBkColor/SetTextColor — разные вызовы для фона и
// текста), cairo_set_source_rgb(a) — ОДИН и тот же вызов что для фона, что
// для текста, что для линий. Различить их по имени функции нельзя, поэтому
// используем единую эвристику по светлоте (lightness): светлый цвет — это
// почти всегда фон -> красим в тёмный; тёмный нейтральный цвет — это почти
// всегда текст -> красим в светлый; всё остальное (насыщенные акценты,
// выделение) оставляем как есть, сохраняя оттенок.
//
// Известное ограничение MVP (см. CLAUDE.md/ledger): не реализованы приёмы
// более продвинутых итераций этой техники (clip-context для различения
// дерева/редактора, ring-history против двоения текста при повторной
// прорисовке одного глифа разными цветами) — эвристика по светлоте без них
// может давать отдельные визуальные артефакты. Достаточно для MVP.

use dark_inject_common::config::Colors;
use dark_inject_common::iat::{for_each_import_across_process, patch_iat_slot};
use std::collections::HashSet;
use std::sync::Mutex;

static ACTIVE_COLORS: Mutex<Colors> = Mutex::new(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
// IAT-слоты, которые мы уже патчили. 1С подгружает часть модулей лениво
// (диалоги свойств, справка и т.п. — см. известное ограничение в комментарии
// к rescan()), и разовый install() при инъекции их не видит, потому что они
// ещё не загружены. Без этого множества повторный скан перепатчил бы уже
// подмененный слот, приняв наш же hook-указатель за "оригинальную" функцию —
// вызов ушёл бы в бесконечную рекурсию.
static PATCHED_SLOTS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

pub fn set_active_colors(colors: Colors) {
    *ACTIVE_COLORS.lock().unwrap() = colors;
}

type CairoT = isize;
type SetSourceRgbFn = extern "system" fn(CairoT, f64, f64, f64);
type SetSourceRgbaFn = extern "system" fn(CairoT, f64, f64, f64, f64);

static mut REAL_SET_SOURCE_RGB: Option<SetSourceRgbFn> = None;
static mut REAL_SET_SOURCE_RGBA: Option<SetSourceRgbaFn> = None;

fn colorref_to_f64(color: u32) -> (f64, f64, f64) {
    // COLORREF = 0x00BBGGRR
    let r = (color & 0xFF) as f64 / 255.0;
    let g = ((color >> 8) & 0xFF) as f64 / 255.0;
    let b = ((color >> 16) & 0xFF) as f64 / 255.0;
    (r, g, b)
}

fn lightness(r: f64, g: f64, b: f64) -> f64 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    (max + min) / 2.0
}

/// Возвращает (r,g,b) для реальной отрисовки: подменённый цвет для
/// светлых/тёмных нейтральных цветов, оригинал — для насыщенных акцентов
/// (средняя светлота — обычно значит высокая насыщенность где-то посередине,
/// либо намеренный акцентный цвет, который лучше не трогать).
fn remap(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let l = lightness(r, g, b);
    let colors = *ACTIVE_COLORS.lock().unwrap();
    if l > 0.6 {
        colorref_to_f64(colors.bg)
    } else if l < 0.35 {
        colorref_to_f64(colors.text)
    } else {
        (r, g, b)
    }
}

extern "system" fn hook_set_source_rgb(cr: CairoT, r: f64, g: f64, b: f64) {
    let (r, g, b) = remap(r, g, b);
    unsafe { REAL_SET_SOURCE_RGB.expect("hook installed without real fn")(cr, r, g, b) }
}

extern "system" fn hook_set_source_rgba(cr: CairoT, r: f64, g: f64, b: f64, a: f64) {
    let (r, g, b) = remap(r, g, b);
    unsafe { REAL_SET_SOURCE_RGBA.expect("hook installed without real fn")(cr, r, g, b, a) }
}

/// Патчит IAT всех модулей процесса, подменяя cairo_set_source_rgb(a) из
/// cairo.dll на наши обёртки. Реальный вызывающий модуль — не сам 1cv8.exe, а
/// grphcs.dll (внутренний "мост" 1С к cairo), поэтому сканируем ВСЕ модули
/// процесса, а не только главный exe.
///
/// Идемпотентно: уже пропатченные слоты пропускаются (см. PATCHED_SLOTS) —
/// безопасно вызывать многократно. Это важно, потому что 1С подгружает часть
/// модулей ЛЕНИВО (диалоги свойств объектов, справка и т.п. появляются уже
/// после инъекции) — разовый вызов при старте их не видит. Вызывающий код
/// (worker.rs) должен периодически вызывать rescan() повторно, чтобы поймать
/// такие поздние модули — это тот же приём, что описан во внешнем источнике
/// как "EAT-патч для поздних модулей", только через периодический IAT-скан
/// вместо патча таблицы экспорта самой cairo.dll (проще реализовать, ценой
/// небольшой задержки — не мгновенно с нулевой секунды, а на следующем тике).
pub fn rescan() {
    let mut patched = PATCHED_SLOTS.lock().unwrap();
    let seen = patched.get_or_insert_with(HashSet::new);
    unsafe {
        for_each_import_across_process(
            "cairo.dll",
            &["cairo_set_source_rgb", "cairo_set_source_rgba"],
            |name, slot| {
                let key = slot as usize;
                if !seen.insert(key) {
                    return; // уже пропатчен в прошлый раз
                }
                match name {
                    "cairo_set_source_rgb" => {
                        let original = patch_iat_slot(slot, hook_set_source_rgb as *const () as usize);
                        REAL_SET_SOURCE_RGB = Some(std::mem::transmute::<usize, SetSourceRgbFn>(original));
                    }
                    "cairo_set_source_rgba" => {
                        let original = patch_iat_slot(slot, hook_set_source_rgba as *const () as usize);
                        REAL_SET_SOURCE_RGBA = Some(std::mem::transmute::<usize, SetSourceRgbaFn>(original));
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
    fn remaps_light_colors_to_configured_dark_bg() {
        set_active_colors(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
        let (r, g, b) = remap(1.0, 1.0, 1.0); // белый
        let expected = colorref_to_f64(0x1E1E1E);
        assert!((r - expected.0).abs() < 0.01);
        assert!((g - expected.1).abs() < 0.01);
        assert!((b - expected.2).abs() < 0.01);
    }

    #[test]
    fn remaps_dark_colors_to_configured_light_text() {
        set_active_colors(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
        let (r, g, b) = remap(0.0, 0.0, 0.0); // чёрный
        let expected = colorref_to_f64(0xD4D4D4);
        assert!((r - expected.0).abs() < 0.01);
        assert!((g - expected.1).abs() < 0.01);
        assert!((b - expected.2).abs() < 0.01);
    }

    #[test]
    fn leaves_saturated_accent_colors_unchanged() {
        set_active_colors(Colors { bg: 0x1E1E1E, text: 0xD4D4D4, line: 0x3C3C3C });
        // Насыщенный красный: lightness = (1.0+0.0)/2 = 0.5 — средняя зона, не трогаем.
        let (r, g, b) = remap(1.0, 0.0, 0.0);
        assert_eq!((r, g, b), (1.0, 0.0, 0.0));
    }

    #[test]
    fn finds_real_cairo_imports_if_cairo_is_loaded() {
        // В отличие от gdi32-теста, здесь не форсируем вызов — cairo.dll
        // обычно не загружена в обычном тестовом процессе, и это нормально:
        // тест просто проверяет, что сканер не падает и работает как no-op,
        // когда целевой DLL нет ни в одном модуле процесса.
        let mut found = Vec::new();
        unsafe {
            for_each_import_across_process(
                "cairo.dll",
                &["cairo_set_source_rgb", "cairo_set_source_rgba"],
                |name, slot| {
                    found.push((name.to_string(), slot as usize));
                },
            );
        }
        // Не утверждаем непустой результат — только то, что вызов завершился
        // без падения. Реальная проверка нахождения импортов — на живом
        // 1cv8.exe (см. CLAUDE.md/ledger).
        let _ = found;
    }
}
