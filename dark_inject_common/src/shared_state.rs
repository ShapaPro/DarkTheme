use crate::win32::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct SharedFlag {
    _handle: HANDLE,
    view: *mut AtomicU32,
}

unsafe impl Send for SharedFlag {}
unsafe impl Sync for SharedFlag {}

impl SharedFlag {
    /// name — например "Local\\DarkInject1C_State". Создаёт mapping, если не
    /// существует, иначе открывает существующий (все процессы на одной сессии
    /// Windows, вызвавшие open_or_create с тем же именем, разделяют одну и ту
    /// же память).
    pub fn open_or_create(name: &str) -> Result<SharedFlag, String> {
        let wide = to_wide(name);
        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null_mut(),
                PAGE_READWRITE,
                0,
                std::mem::size_of::<u32>() as u32,
                wide.as_ptr(),
            );
            if handle == 0 {
                return Err("CreateFileMappingW failed".to_string());
            }
            let view = MapViewOfFile(handle, FILE_MAP_WRITE | FILE_MAP_READ, 0, 0, std::mem::size_of::<u32>());
            if view.is_null() {
                CloseHandle(handle);
                return Err("MapViewOfFile failed".to_string());
            }
            Ok(SharedFlag {
                _handle: handle,
                view: view as *mut AtomicU32,
            })
        }
    }

    pub fn get(&self) -> bool {
        unsafe { (*self.view).load(Ordering::SeqCst) != 0 }
    }

    pub fn set(&self, value: bool) {
        unsafe { (*self.view).store(if value { 1 } else { 0 }, Ordering::SeqCst) };
    }
}

impl Drop for SharedFlag {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.view as *const _);
            CloseHandle(self._handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_across_two_handles_same_name() {
        let name = "Local\\DarkInject1C_Test_Roundtrip";
        let writer = SharedFlag::open_or_create(name).expect("writer");
        let reader = SharedFlag::open_or_create(name).expect("reader");

        writer.set(true);
        assert!(reader.get(), "reader should see writer's true");

        writer.set(false);
        assert!(!reader.get(), "reader should see writer's false");
    }

    #[test]
    fn defaults_to_false_on_fresh_mapping() {
        let name = "Local\\DarkInject1C_Test_Fresh";
        let flag = SharedFlag::open_or_create(name).expect("flag");
        assert!(!flag.get());
    }
}
