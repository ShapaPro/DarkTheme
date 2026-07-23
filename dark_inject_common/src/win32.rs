#![allow(non_snake_case, non_camel_case_types, dead_code)]
use std::ffi::c_void;

pub type HWND = isize;
pub type HANDLE = isize;
pub type HMODULE = isize;
pub type HINSTANCE = isize;
pub type HHOOK = isize;

pub const INVALID_HANDLE_VALUE: isize = -1;

// --- Toolhelp32 ---
pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
pub const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;

#[repr(C)]
pub struct PROCESSENTRY32W {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ProcessID: u32,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: u32,
    pub cntThreads: u32,
    pub th32ParentProcessID: u32,
    pub pcPriClassBase: i32,
    pub dwFlags: u32,
    pub szExeFile: [u16; 260],
}

impl Default for PROCESSENTRY32W {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
pub struct THREADENTRY32 {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ThreadID: u32,
    pub th32OwnerProcessID: u32,
    pub tpBasePri: i32,
    pub tpDeltaPri: i32,
    pub dwFlags: u32,
}

impl Default for THREADENTRY32 {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// --- Process access rights ---
pub const PROCESS_CREATE_THREAD: u32 = 0x0002;
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_VM_OPERATION: u32 = 0x0008;
pub const PROCESS_VM_WRITE: u32 = 0x0020;
pub const PROCESS_VM_READ: u32 = 0x0010;

// --- Memory ---
pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_RESERVE: u32 = 0x2000;
pub const MEM_RELEASE: u32 = 0x8000;
pub const PAGE_READWRITE: u32 = 0x04;

// --- File mapping (shared state) ---
pub const FILE_MAP_WRITE: u32 = 0x0002;
pub const FILE_MAP_READ: u32 = 0x0004;

// --- Common control messages (значения из CLAUDE.md, уже проверенные ранее) ---
pub const TVM_SETBKCOLOR: u32 = 0x111D;
pub const TVM_SETTEXTCOLOR: u32 = 0x1120;
pub const TVM_SETLINECOLOR: u32 = 0x1128;
pub const LVM_SETBKCOLOR: u32 = 0x1001;
pub const LVM_SETTEXTCOLOR: u32 = 0x1024;
pub const LVM_SETTEXTBKCOLOR: u32 = 0x1026;
// HDM_SETBKCOLOR — best-effort, не во всех версиях comctl32 задокументировано одинаково.
pub const HDM_SETBKCOLOR: u32 = 0x1201;

// --- DWM ---
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;

// --- Window styles / hooks ---
pub const GWL_STYLE: i32 = -16;
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WH_CBT: i32 = 5;
pub const HCBT_CREATEWND: i32 = 3;

// --- comctl32 init flags ---
pub const ICC_LISTVIEW_CLASSES: u32 = 0x0000_0001; // listview + header
pub const ICC_TREEVIEW_CLASSES: u32 = 0x0000_0002;

#[repr(C)]
pub struct INITCOMMONCONTROLSEX {
    pub dwSize: u32,
    pub dwICC: u32,
}

// --- Window creation for tests ---
pub const HWND_MESSAGE: isize = -3;

extern "system" {
    // kernel32
    pub fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> HANDLE;
    pub fn Process32FirstW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> i32;
    pub fn Process32NextW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> i32;
    pub fn Thread32First(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> i32;
    pub fn Thread32Next(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> i32;
    pub fn CloseHandle(hObject: HANDLE) -> i32;
    pub fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> HANDLE;
    pub fn VirtualAllocEx(
        hProcess: HANDLE,
        lpAddress: *mut c_void,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut c_void;
    pub fn VirtualFreeEx(hProcess: HANDLE, lpAddress: *mut c_void, dwSize: usize, dwFreeType: u32) -> i32;
    pub fn WriteProcessMemory(
        hProcess: HANDLE,
        lpBaseAddress: *mut c_void,
        lpBuffer: *const c_void,
        nSize: usize,
        lpNumberOfBytesWritten: *mut usize,
    ) -> i32;
    pub fn CreateRemoteThread(
        hProcess: HANDLE,
        lpThreadAttributes: *mut c_void,
        dwStackSize: usize,
        lpStartAddress: *const c_void,
        lpParameter: *mut c_void,
        dwCreationFlags: u32,
        lpThreadId: *mut u32,
    ) -> HANDLE;
    pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: u32) -> u32;
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> *const c_void;
    pub fn CreateThread(
        lpThreadAttributes: *mut c_void,
        dwStackSize: usize,
        lpStartAddress: extern "system" fn(*mut c_void) -> u32,
        lpParameter: *mut c_void,
        dwCreationFlags: u32,
        lpThreadId: *mut u32,
    ) -> HANDLE;
    pub fn CreateFileMappingW(
        hFile: HANDLE,
        lpAttributes: *mut c_void,
        flProtect: u32,
        dwMaximumSizeHigh: u32,
        dwMaximumSizeLow: u32,
        lpName: *const u16,
    ) -> HANDLE;
    pub fn MapViewOfFile(
        hFileMappingObject: HANDLE,
        dwDesiredAccess: u32,
        dwFileOffsetHigh: u32,
        dwFileOffsetLow: u32,
        dwNumberOfBytesToMap: usize,
    ) -> *mut c_void;
    pub fn UnmapViewOfFile(lpBaseAddress: *const c_void) -> i32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentThreadId() -> u32;
    pub fn GetModuleFileNameW(hModule: HMODULE, lpFilename: *mut u16, nSize: u32) -> u32;

    // user32
    pub fn EnumWindows(
        lpEnumFunc: extern "system" fn(HWND, isize) -> i32,
        lParam: isize,
    ) -> i32;
    pub fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut u32) -> u32;
    pub fn GetClassNameW(hWnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
    pub fn SendMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> isize;
    pub fn SetWindowsHookExW(
        idHook: i32,
        lpfn: extern "system" fn(i32, usize, isize) -> isize,
        hmod: HINSTANCE,
        dwThreadId: u32,
    ) -> HHOOK;
    pub fn CallNextHookEx(hhk: HHOOK, nCode: i32, wParam: usize, lParam: isize) -> isize;
    pub fn UnhookWindowsHookEx(hhk: HHOOK) -> i32;
    pub fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> isize;
    pub fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        x: i32,
        y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: isize,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;
    pub fn DestroyWindow(hWnd: HWND) -> i32;

    // dwmapi
    pub fn DwmSetWindowAttribute(
        hwnd: HWND,
        dwAttribute: u32,
        pvAttribute: *const c_void,
        cbAttribute: u32,
    ) -> i32;

    // uxtheme
    pub fn SetWindowTheme(hwnd: HWND, pszSubAppName: *const u16, pszSubIdList: *const u16) -> i32;

    // comctl32
    pub fn InitCommonControlsEx(picce: *const INITCOMMONCONTROLSEX) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {}
#[link(name = "uxtheme")]
extern "system" {}
#[link(name = "comctl32")]
extern "system" {}

pub fn to_wide(s: &str) -> Vec<u16> {
    use std::iter::once;
    s.encode_utf16().chain(once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_wide() {
        assert_eq!(to_wide("abc"), vec![97u16, 98, 99, 0]);
    }
}
