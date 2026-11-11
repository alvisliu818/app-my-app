//! 手写 Win32 FFI：只声明本程序用到的 API，避免引入 windows-sys 的重型特性集。
#![allow(non_snake_case, dead_code)]

use std::ffi::c_void;

pub type HWND = isize;
pub type HDC = isize;
pub type HICON = isize;
pub type HMENU = isize;
pub type HINSTANCE = isize;
pub type HHOOK = isize;
pub type HKEY = isize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;

pub type WNDPROC = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
pub type HOOKPROC = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;
pub type ENUMWINDOWSPROC = unsafe extern "system" fn(HWND, LPARAM) -> i32;
pub type MONITORENUMPROC = unsafe extern "system" fn(isize, HDC, *mut RECT, LPARAM) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct POINT { pub x: i32, pub y: i32 }
impl Default for POINT { fn default() -> Self { unsafe { std::mem::zeroed() } } }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RECT { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }
impl Default for RECT { fn default() -> Self { unsafe { std::mem::zeroed() } } }

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
    pub lPrivate: u32,
}
impl Default for MSG { fn default() -> Self { unsafe { std::mem::zeroed() } } }

#[repr(C)]
pub struct WNDCLASSW {
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: isize,
    pub hbrBackground: isize,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
}

#[repr(C)]
pub struct MONITORINFO {
    pub cbSize: u32,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: u32,
}
impl Default for MONITORINFO { fn default() -> Self { unsafe { std::mem::zeroed() } } }

#[repr(C)]
pub struct MSLLHOOKSTRUCT {
    pub pt: POINT,
    pub mouseData: u32,
    pub flags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
pub struct GUID { pub data1: u32, pub data2: u16, pub data3: u16, pub data4: [u8; 8] }
impl Default for GUID { fn default() -> Self { unsafe { std::mem::zeroed() } } }

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uTimeoutVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: GUID,
    pub hBalloonIcon: HICON,
}
impl Default for NOTIFYICONDATAW { fn default() -> Self { unsafe { std::mem::zeroed() } } }

// 常用常量
pub const NULL: isize = 0;
pub const HWND_MESSAGE: HWND = -3;
pub const HWND_TOP: HWND = 0;
pub const HWND_BOTTOM: HWND = 1;
pub const HWND_TOPMOST: HWND = -1;
pub const HWND_NOTOPMOST: HWND = -2;

pub const GWL_STYLE: i32 = -16;
pub const GWL_EXSTYLE: i32 = -20;
pub const GWLP_USERDATA: i32 = -21;
pub const WS_POPUP: isize = 0x8000_0000u32 as isize;
pub const WS_OVERLAPPEDWINDOW: isize = 0x00CF_0000;
pub const WS_THICKFRAME: isize = 0x0004_0000;
pub const WS_CAPTION: isize = 0x00C0_0000;
pub const WS_EX_TOPMOST: isize = 0x8;
pub const WS_EX_TOOLWINDOW: isize = 0x80;
pub const WS_EX_LAYERED: isize = 0x0008_0000;
pub const WS_EX_TRANSPARENT: isize = 0x20;

pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SWP_NOZORDER: u32 = 0x0004;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const SWP_FRAMECHANGED: u32 = 0x0020;

pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const SW_SHOW: i32 = 5;
pub const SW_RESTORE: i32 = 9;

pub const WM_APP: u32 = 0x8000;
pub const WM_NULL: u32 = 0;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUIT: u32 = 0x0012;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_HOTKEY: u32 = 0x0312;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MOUSEMOVE: u32 = 0x0200;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MOD_NOREPEAT: u32 = 0x4000;
pub const VK_CONTROL: i32 = 0x11;
pub const VK_LBUTTON: i32 = 0x01;
pub const VK_LEFT: u32 = 0x25;
pub const VK_UP: u32 = 0x26;
pub const VK_RIGHT: u32 = 0x27;
pub const VK_DOWN: u32 = 0x28;
pub const WH_MOUSE_LL: i32 = 14;
pub const HC_ACTION: i32 = 0;
pub const WM_REFRESH: u32 = WM_APP + 2;
pub const WM_DETECT: u32 = WM_APP + 3;
pub const TIMER_GUARD: usize = 1;
pub const WM_GETHOTKEY: u32 = 0x0033;
pub const SMTO_ABORTIFHUNG: u32 = 0x0002;
pub const SMTO_BLOCK: u32 = 0x0001;
pub const HOTKEYF_SHIFT: u32 = 0x01;
pub const HOTKEYF_CONTROL: u32 = 0x02;
pub const HOTKEYF_ALT: u32 = 0x04;
pub const HOTKEYF_EXT: u32 = 0x08;
/// 注册热键时使用的稳定 ID：BASE + 动作在 ACTIONS 中的下标
pub const HOTKEY_ID_BASE: i32 = 1000;
/// 探测占用时临时使用的 ID（与正式注册不冲突）
pub const PROBE_ID_BASE: i32 = 5000;

pub const MONITOR_DEFAULTTONEAREST: u32 = 2;
pub const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
pub const SPI_GETWORKAREA: u32 = 0x0030;
pub const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const GA_ROOT: u32 = 2;
pub const GW_HWNDPREV: u32 = 3;

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIF_MESSAGE: u32 = 1;
pub const NIF_ICON: u32 = 2;
pub const NIF_TIP: u32 = 4;
pub const IDI_APPLICATION: isize = 32512;

pub const MF_STRING: u32 = 0;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_UNCHECKED: u32 = 0;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;

pub const MB_OK: u32 = 0;
pub const MB_ICONINFORMATION: u32 = 0x40;
pub const MB_TOPMOST: u32 = 0x0004_0000;

// 64 位下 HKEY_CURRENT_USER 是 (LONG)0x80000001 符号扩展后的指针值
pub const HKEY_CURRENT_USER: HKEY = 0xFFFF_FFFF_8000_0001u64 as isize;
pub const KEY_READ: u32 = 0x20019;
pub const KEY_WRITE: u32 = 0x20006;
pub const REG_SZ: u32 = 1;
pub const ERROR_SUCCESS: i32 = 0;

// 菜单命令 ID
pub const TRAY_MSG: u32 = WM_APP + 1;
pub const ID_SHOW: u32 = 100;
pub const ID_LAYOUT_BASE: u32 = 200;
pub const ID_CAPTURE: u32 = 300;
pub const ID_ARR_LEFT: u32 = 401;
pub const ID_ARR_RIGHT: u32 = 402;
pub const ID_ARR_FULL: u32 = 403;
pub const ID_RESTORE: u32 = 404;
pub const ID_INFO: u32 = 405;
pub const ID_AUTOSTART: u32 = 406;
pub const ID_GUARD: u32 = 407;
pub const ID_LOG: u32 = 408;
pub const ID_HOTKEYS: u32 = 409;
pub const ID_QUIT: u32 = 999;

pub const HOT_LAYOUT_BASE: i32 = 100; // Win+Alt+Shift+1..9
pub const HOT_LEFT: i32 = 201;
pub const HOT_RIGHT: i32 = 202;
pub const HOT_FULL: i32 = 203;
pub const HOT_RESTORE: i32 = 204;
pub const HOT_LOCK: i32 = 205;
pub const HOT_GUARD: i32 = 206;
pub const HOT_INFO: i32 = 207;
pub const HOT_CAPTURE: i32 = 208;
// 旧版 WindowHalf 键位（Win+Alt+1/2/3/4）
pub const HOT_LEGACY_BASE: i32 = 211;

// ---------------- 字符串工具 ----------------
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
pub fn low_word(v: isize) -> u16 { (v as usize & 0xFFFF) as u16 }
pub fn hi_word(v: isize) -> u16 { ((v as usize >> 16) & 0xFFFF) as u16 }

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassW(lpWndClass: *const WNDCLASSW) -> u16;
    pub fn CreateWindowExW(dwExStyle: u32, lpClassName: *const u16, lpWindowName: *const u16,
        dwStyle: u32, x: i32, y: i32, nWidth: i32, nHeight: i32,
        hWndParent: HWND, hMenu: HMENU, hInstance: HINSTANCE, lpParam: LPARAM) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn GetMessageW(lpMsg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    pub fn PeekMessageW(lpMsg: *mut MSG, hwnd: HWND, min: u32, max: u32, remove: u32) -> i32;
    pub fn TranslateMessage(lpMsg: *const MSG) -> i32;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostQuitMessage(nExitCode: i32);
    pub fn PostMessageW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> i32;
    pub fn SendMessageTimeoutW(hwnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM,
        fuFlags: u32, uTimeout: u32, lpdwResult: *mut usize) -> LRESULT;
    pub fn DestroyWindow(hwnd: HWND) -> i32;
    pub fn ShowWindow(hwnd: HWND, nCmdShow: i32) -> i32;
    pub fn IsWindow(hwnd: HWND) -> i32;
    pub fn IsWindowVisible(hwnd: HWND) -> i32;
    pub fn IsIconic(hwnd: HWND) -> i32;
    pub fn IsZoomed(hwnd: HWND) -> i32;
    pub fn GetForegroundWindow() -> HWND;
    pub fn SetForegroundWindow(hwnd: HWND) -> i32;
    pub fn BringWindowToTop(hwnd: HWND) -> i32;
    pub fn GetWindow(hwnd: HWND, uCmd: u32) -> HWND;
    pub fn GetDesktopWindow() -> HWND;
    pub fn GetAncestor(hwnd: HWND, gaFlags: u32) -> HWND;
    pub fn WindowFromPoint(pt: POINT) -> HWND;
    pub fn EnumWindows(lpEnumFunc: ENUMWINDOWSPROC, lParam: LPARAM) -> i32;
    pub fn GetWindowThreadProcessId(hwnd: HWND, lpdwProcessId: *mut u32) -> u32;
    pub fn GetClassNameW(hwnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
    pub fn GetWindowTextW(hwnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: HWND) -> i32;
    pub fn GetWindowRect(hwnd: HWND, lpRect: *mut RECT) -> i32;
    pub fn GetWindowLongPtrW(hwnd: HWND, nIndex: i32) -> isize;
    pub fn SetWindowLongPtrW(hwnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    pub fn SetWindowPos(hwnd: HWND, hWndInsertAfter: HWND, x: i32, y: i32, cx: i32, cy: i32, uFlags: u32) -> i32;
    pub fn MoveWindow(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, repaint: i32) -> i32;
    pub fn MonitorFromWindow(hwnd: HWND, dwFlags: u32) -> isize;
    pub fn MonitorFromPoint(pt: POINT, dwFlags: u32) -> isize;
    pub fn GetMonitorInfoW(hMonitor: isize, lpmi: *mut MONITORINFO) -> i32;
    pub fn EnumDisplayMonitors(hdc: HDC, lprcClip: *const RECT, lpfnEnum: MONITORENUMPROC, dwData: LPARAM) -> i32;
    pub fn SystemParametersInfoW(uiAction: u32, uiParam: u32, pvParam: *mut c_void, fWinIni: u32) -> i32;
    pub fn GetSystemMetrics(nIndex: i32) -> i32;
    pub fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    pub fn GetDpiForWindow(hwnd: HWND) -> u32;
    pub fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    pub fn ClipCursor(lpRect: *const RECT) -> i32;
    pub fn GetAsyncKeyState(vKey: i32) -> i16;
    pub fn RegisterHotKey(hwnd: HWND, id: i32, fsModifiers: u32, vk: u32) -> i32;
    pub fn UnregisterHotKey(hwnd: HWND, id: i32) -> i32;
    pub fn SetWindowsHookExW(idHook: i32, lpfn: HOOKPROC, hmod: HINSTANCE, dwThreadId: u32) -> HHOOK;
    pub fn UnhookWindowsHookEx(hhk: HHOOK) -> i32;
    pub fn CallNextHookEx(hhk: HHOOK, nCode: i32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn SetTimer(hwnd: HWND, nIDEvent: usize, uElapse: u32, lpTimerFunc: usize) -> usize;
    pub fn KillTimer(hwnd: HWND, nIDEvent: usize) -> i32;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(hMenu: HMENU, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> i32;
    pub fn CheckMenuItem(hMenu: HMENU, uIDCheckItem: u32, uCheck: u32) -> i32;
    pub fn TrackPopupMenuEx(hMenu: HMENU, uFlags: u32, x: i32, y: i32, hwnd: HWND, lptpm: *mut c_void) -> u32;
    pub fn DestroyMenu(hMenu: HMENU) -> i32;
    pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: isize) -> HICON;
    pub fn MessageBoxW(hwnd: HWND, lpText: *const u16, lpCaption: *const u16, uType: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HINSTANCE;
    pub fn GetModuleFileNameW(hModule: HINSTANCE, lpFilename: *mut u16, nSize: u32) -> u32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
    pub fn TerminateProcess(hProcess: isize, uExitCode: u32) -> i32;
    pub fn QueryFullProcessImageNameW(hProcess: isize, dwFlags: u32, lpExeName: *mut u16, lpdwSize: *mut u32) -> i32;
    pub fn CloseHandle(hObject: isize) -> i32;
    pub fn GetLastError() -> u32;
    pub fn SetLastError(dwErrCode: u32);
    pub fn GetTickCount64() -> u64;
    pub fn Sleep(dwMilliseconds: u32);
    pub fn AttachConsole(dwProcessId: u32) -> i32;
    pub fn FreeConsole() -> i32;
}

#[link(name = "shell32")]
extern "system" {
    pub fn ShellExecuteW(hwnd: HWND, lpOperation: *const u16, lpFile: *const u16,
        lpParameters: *const u16, lpDirectory: *const u16, nShowCmd: i32) -> isize;
    pub fn Shell_NotifyIconW(dwMessage: u32, lpData: *mut NOTIFYICONDATAW) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegOpenKeyExW(hKey: HKEY, lpSubKey: *const u16, ulOptions: u32, samDesired: u32, phkResult: *mut HKEY) -> i32;
    pub fn RegSetValueExW(hKey: HKEY, lpValueName: *const u16, reserved: u32, dwType: u32,
        lpData: *const u8, cbData: u32) -> i32;
    pub fn RegQueryValueExW(hKey: HKEY, lpValueName: *const u16, lpReserved: *mut u32,
        lpType: *mut u32, lpData: *mut u8, lpcbData: *mut u32) -> i32;
    pub fn RegDeleteValueW(hKey: HKEY, lpValueName: *const u16) -> i32;
    pub fn RegCloseKey(hKey: HKEY) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmGetWindowAttribute(hwnd: HWND, dwAttribute: u32, pvAttribute: *mut c_void, cbAttribute: u32) -> i32;
}
