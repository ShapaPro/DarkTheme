// Общий разбор PE/IAT для перехвата функций, вызываемых целевым процессом.
// Используется gdihook.rs (перекраска через gdi32) и cairohook.rs
// (перекраска через cairo — реальный движок отрисовки 1С, см. CLAUDE.md
// "Главная находка": стандартных Windows-контролов в 1С нет, весь UI рисует
// сама 1С через cairo.dll/grphcs.dll).
use crate::win32::*;
use std::ffi::c_void;

/// Разбор PE-заголовков модуля вручную (offset-based, без полного
/// #[repr(C)]-описания IMAGE_NT_HEADERS64 — его размер зависит от
/// NumberOfRvaAndSizes, поэтому надёжнее читать нужные поля по известным,
/// стабильным смещениям). Возвращает (rva, size) directory entry import-таблицы.
unsafe fn import_directory(module_base: *const u8) -> Option<(u32, u32)> {
    let e_lfanew = *(module_base.add(0x3C) as *const i32);
    let nt_header = module_base.add(e_lfanew as usize);
    let signature = *(nt_header as *const u32);
    if signature != 0x0000_4550 {
        // "PE\0\0"
        return None;
    }
    // OptionalHeader начинается через 4 (Signature) + 20 (IMAGE_FILE_HEADER) байт.
    let optional_header = nt_header.add(4 + 20);
    let magic = *(optional_header as *const u16);
    if magic != 0x20b {
        // PE32+ (x64). Это x64-проект (см. CLAUDE.md), 32-битный образ не поддерживаем.
        return None;
    }
    // DataDirectory начинается со смещения 112 внутри OptionalHeader64;
    // IMAGE_DIRECTORY_ENTRY_IMPORT = индекс 1, т.е. ещё +8 байт.
    let import_dir_ptr = optional_header.add(112 + 8);
    let rva = *(import_dir_ptr as *const u32);
    let size = *(import_dir_ptr.add(4) as *const u32);
    if rva == 0 || size == 0 {
        None
    } else {
        Some((rva, size))
    }
}

unsafe fn read_cstr(ptr: *const u8) -> String {
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf8_lossy(slice).to_string()
}

#[repr(C)]
struct ImportDescriptor {
    original_first_thunk: u32,
    time_date_stamp: u32,
    forwarder_chain: u32,
    name: u32,
    first_thunk: u32,
}

/// Патчит один IAT-слот (адрес, по которому реально вызывается функция) на
/// наш hook, возвращая исходный указатель для последующего вызова через него.
pub unsafe fn patch_iat_slot(slot: *mut usize, hook: usize) -> usize {
    let mut old_protect: u32 = 0;
    VirtualProtect(
        slot as *mut c_void,
        std::mem::size_of::<usize>(),
        PAGE_EXECUTE_READWRITE,
        &mut old_protect,
    );
    let original = *slot;
    *slot = hook;
    VirtualProtect(slot as *mut c_void, std::mem::size_of::<usize>(), old_protect, &mut old_protect);
    original
}

/// Находит в IAT ОДНОГО заданного модуля (module_base) импорты из target_dll,
/// совпадающие по имени с одним из target_names, и вызывает
/// on_found(name, iat_slot) для каждого совпадения.
pub unsafe fn for_each_import_in_module(
    module_base: HMODULE,
    target_dll: &str,
    target_names: &[&str],
    on_found: &mut dyn FnMut(&str, *mut usize),
) {
    if module_base == 0 {
        return;
    }
    let base_ptr = module_base as *const u8;
    let Some((import_rva, import_size)) = import_directory(base_ptr) else {
        return;
    };
    let descriptor_count = import_size as usize / std::mem::size_of::<ImportDescriptor>();
    let descriptors = base_ptr.add(import_rva as usize) as *const ImportDescriptor;

    for i in 0..descriptor_count {
        let desc = &*descriptors.add(i);
        if desc.name == 0 {
            break;
        }
        let dll_name = read_cstr(base_ptr.add(desc.name as usize));
        if !dll_name.eq_ignore_ascii_case(target_dll) {
            continue;
        }
        if desc.original_first_thunk == 0 {
            // Импорты без INT (только IAT) — по именам восстановить нечего,
            // пропускаем эту библиотеку в этом модуле.
            continue;
        }

        let int_base = base_ptr.add(desc.original_first_thunk as usize) as *const u64;
        let iat_base = base_ptr.add(desc.first_thunk as usize) as *mut usize;

        let mut idx = 0isize;
        loop {
            let int_entry = *int_base.offset(idx);
            if int_entry == 0 {
                break;
            }
            if int_entry & 0x8000_0000_0000_0000 == 0 {
                // RVA на IMAGE_IMPORT_BY_NAME { Hint: u16, Name: char[] }.
                let name_ptr = base_ptr.add(int_entry as usize + 2);
                let func_name = read_cstr(name_ptr);
                if let Some(&matched) = target_names.iter().find(|n| **n == func_name) {
                    on_found(matched, iat_base.offset(idx));
                }
            }
            idx += 1;
        }
    }
}

/// Возвращает базовые адреса (HMODULE) всех модулей, загруженных в ТЕКУЩЕМ
/// процессе. Реальные вызовы cairo/gdi могут идти не из главного exe, а из
/// вспомогательной DLL (например grphcs.dll у 1С) — поэтому нужно сканировать
/// все модули, а не только GetModuleHandleW(NULL).
pub fn loaded_modules_of_current_process() -> Vec<HMODULE> {
    let mut result = Vec::new();
    unsafe {
        let pid = GetCurrentProcessId();
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
        if snapshot == INVALID_HANDLE_VALUE {
            return result;
        }
        let mut entry = MODULEENTRY32W::default();
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        if Module32FirstW(snapshot, &mut entry) != 0 {
            loop {
                result.push(entry.hModule);
                if Module32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    result
}

/// Как for_each_import_in_module, но по всем модулям текущего процесса сразу.
pub unsafe fn for_each_import_across_process(
    target_dll: &str,
    target_names: &[&str],
    mut on_found: impl FnMut(&str, *mut usize),
) {
    for module_base in loaded_modules_of_current_process() {
        for_each_import_in_module(module_base, target_dll, target_names, &mut on_found);
    }
}
