//! Win32 窗口操作层：枚举、样式修复、摆放、启动、显示器工作区。
use std::ffi::c_void;
use std::path::Path;

use crate::ffi::*;
use crate::model::*;

#[derive(Debug, Clone, Default)]
pub struct RawWin {
    pub hwnd: i64,
    pub title: String,
    pub class: String,
    pub process: String,
    pub exe: String,
    pub rect: [i32; 4],
    pub style: isize,
    pub ex: isize,
    pub pid: u32,
    pub z: i32,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    let out = &mut *(lparam as *mut Vec<RawWin>);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 { return 1; }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() { return 1; } // 排除自身（Tauri 主窗口 / 系统窗口）

    let mut cls_buf = [0u16; 256];
    GetClassNameW(hwnd, cls_buf.as_mut_ptr(), 256);
    let class = from_wide(&cls_buf);
    if is_system_class(&class) { return 1; }

    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    if (ex & WS_EX_TOOLWINDOW) != 0 { return 1; }

    let n = GetWindowTextLengthW(hwnd);
    let mut title_buf = vec![0u16; (n + 2) as usize];
    GetWindowTextW(hwnd, title_buf.as_mut_ptr(), (n + 2) as i32);
    let title = from_wide(&title_buf);

    let mut rc: RECT = Default::default();
    let mut visual: RECT = Default::default();
    let ok = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut visual as *mut _ as *mut c_void, std::mem::size_of::<RECT>() as u32);
    if ok == 0 && visual.right > visual.left && visual.bottom > visual.top { rc = visual; }
    else { GetWindowRect(hwnd, &mut rc); }
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    if w < 80 || h < 60 { return 1; }

    let (process, exe) = process_path(pid);
    if process.eq_ignore_ascii_case("explorer.exe") && (class == "Progman" || class == "WorkerW" || title.is_empty()) {
        return 1;
    }
    out.push(RawWin {
        hwnd: hwnd as i64, title, class, process, exe,
        rect: [rc.left, rc.top, w, h],
        style: GetWindowLongPtrW(hwnd, GWL_STYLE),
        ex, pid, z: out.len() as i32,
    });
    1
}

fn is_system_class(cls: &str) -> bool {
    const BAD: &[&str] = &[
        "Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd",
        "DuiHostWnd", "Windows.UI.Core.CoreWindow", "NotifyIconOverflowWindow",
        "Tao Thread", "Qt5152QWindowIcon", "Windows.UI.Input.InputSite.WindowClass",
        "Shell_SystemTray", "TaskListThumbnailWnd", "ForegroundStaging",
    ];
    BAD.iter().any(|b| cls.contains(b))
}

pub fn process_path(pid: u32) -> (String, String) {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 { return (String::new(), String::new()); }
        let mut buf = vec![0u16; 32768];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok == 0 { return (String::new(), String::new()); }
        let full = from_wide(&buf[..size as usize]);
        let name = full.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        (name, full)
    }
}

/// EnumWindows 顺序 = Z 序（最前 -> 最后）。
pub fn enum_windows() -> Vec<RawWin> {
    let mut out: Vec<RawWin> = Vec::new();
    unsafe { EnumWindows(enum_cb, &mut out as *mut _ as LPARAM); }
    out
}

pub fn windows_json() -> Vec<WinInfo> {
    enum_windows().into_iter().map(|w| WinInfo {
        hwnd: w.hwnd, title: w.title, class: w.class, process: w.process, exe: w.exe,
        rect: w.rect, z: w.z, visible: true,
    }).collect()
}

pub fn win_by_hwnd(hwnd: i64) -> Option<RawWin> {
    enum_windows().into_iter().find(|w| w.hwnd == hwnd)
}

pub fn primary_work_area() -> RectPx {
    unsafe {
        let mut rc: RECT = Default::default();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rc as *mut _ as *mut c_void, 0) != 0 {
            RectPx { x: rc.left, y: rc.top, w: rc.right - rc.left, h: rc.bottom - rc.top }
        } else {
            RectPx { x: 0, y: 0, w: GetSystemMetrics(0), h: GetSystemMetrics(1) }
        }
    }
}

unsafe extern "system" fn monitor_cb(_hmon: isize, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> i32 {
    let out = &mut *(lparam as *mut Vec<RectPx>);
    let mut mi: MONITORINFO = Default::default();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(_hmon, &mut mi) != 0 {
        let r = mi.rcWork;
        out.push(RectPx { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top });
    }
    1
}

pub fn all_monitors() -> Vec<RectPx> {
    let mut out: Vec<RectPx> = Vec::new();
    unsafe { EnumDisplayMonitors(0, std::ptr::null(), monitor_cb, &mut out as *mut _ as LPARAM); }
    if out.is_empty() { out.push(primary_work_area()); }
    out
}

pub fn work_area_for(hwnd: i64) -> RectPx {
    unsafe {
        let hmon = MonitorFromWindow(hwnd as HWND, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = Default::default();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if hmon != 0 && GetMonitorInfoW(hmon, &mut mi) != 0 {
            let r = mi.rcWork;
            return RectPx { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
        }
    }
    primary_work_area()
}

pub fn work_area_containing(rect: [i32; 4]) -> RectPx {
    let pt = POINT { x: rect[0] + rect[2] / 2, y: rect[1] + rect[3] / 2 };
    unsafe {
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = Default::default();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if hmon != 0 && GetMonitorInfoW(hmon, &mut mi) != 0 {
            let r = mi.rcWork;
            return RectPx { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
        }
    }
    primary_work_area()
}

/// 归一化矩形 -> 屏幕像素矩形（按目标显示器工作区）。
pub fn rect_to_px(norm: [f64; 4], wa: &RectPx) -> [i32; 4] {
    [
        wa.x + (norm[0] * wa.w as f64).round() as i32,
        wa.y + (norm[1] * wa.h as f64).round() as i32,
        (norm[2] * wa.w as f64).round() as i32,
        (norm[3] * wa.h as f64).round() as i32,
    ]
}

pub fn rect_to_norm(r: [i32; 4], wa: &RectPx) -> [f64; 4] {
    let f = |v: f64| (v * 10000.0).round() / 10000.0;
    if wa.w <= 0 || wa.h <= 0 { return [0.0, 0.0, 1.0, 1.0]; }
    [f((r[0] - wa.x) as f64 / wa.w as f64), f((r[1] - wa.y) as f64 / wa.h as f64),
     f(r[2] as f64 / wa.w as f64), f(r[3] as f64 / wa.h as f64)]
}

/// 修复无边框/POPUP 窗口（WindowHalf 原有核心能力）。
/// 记录原始 style/ex/rect 以支持「还原原状」。
pub fn fix_style(hwnd: i64, base: &Path, store: &mut Store) -> bool {
    unsafe {
        let h = hwnd as HWND;
        if IsWindow(h) == 0 { return false; }
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let mut rc: RECT = Default::default();
        GetWindowRect(h, &mut rc);
        let cur = [rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top];
        let key = hwnd.to_string();
        if !store.originals.contains_key(&key) {
            store.originals.insert(key.clone(), Original { style: style as i64, ex: ex as i64, rect: cur });
            log_line(base, &format!("保存原始状态 hwnd={} style=0x{:X} rect={:?}", hwnd, style, cur));
        }
        let mut changed = false;
        if (style & WS_OVERLAPPEDWINDOW) != WS_OVERLAPPEDWINDOW || (style & WS_POPUP) != 0 {
            let new_style = (style & !WS_POPUP) | WS_OVERLAPPEDWINDOW;
            SetLastError(0);
            SetWindowLongPtrW(h, GWL_STYLE, new_style);
            let err = GetLastError();
            SetWindowPos(h, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
            log_line(base, &format!("fix_style hwnd={} 0x{:X} -> 0x{:X} err={}", hwnd, style, new_style, err));
            changed = true;
        }
        if (ex & WS_EX_TOPMOST) != 0 {
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex & !WS_EX_TOPMOST);
            SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            changed = true;
        }
        if IsIconic(h) != 0 || IsZoomed(h) != 0 { ShowWindow(h, SW_RESTORE); }
        ClipCursor(std::ptr::null());
        changed
    }
}

pub fn restore_style(hwnd: i64, o: &Original) {
    unsafe {
        let h = hwnd as HWND;
        if IsWindow(h) == 0 { return; }
        SetWindowLongPtrW(h, GWL_STYLE, o.style as isize);
        SetWindowLongPtrW(h, GWL_EXSTYLE, o.ex as isize);
        SetWindowPos(h, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        if (o.ex & WS_EX_TOPMOST as i64) != 0 {
            SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        } else {
            SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        MoveWindow(h, o.rect[0], o.rect[1], o.rect[2], o.rect[3], 1);
    }
}

/// 可视边界（DWM Extended Frame Bounds）相对窗口矩形的外扩量 [left, top, right, bottom]。
/// Win10/11 窗口四周有 ~8px 隐形缩放边框：SetWindowPos 设的是含边框的外矩形，
/// 可见内容比目标矩形小一圈——相邻窗口之间因此出现空隙。摆位后按此补偿即可无缝。
pub fn frame_inset(hwnd: i64) -> [i32; 4] {
    unsafe {
        let h = hwnd as HWND;
        let mut wr: RECT = Default::default();
        let mut vr: RECT = Default::default();
        GetWindowRect(h, &mut wr);
        let ok = DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut vr as *mut _ as *mut c_void, std::mem::size_of::<RECT>() as u32);
        if ok != 0 || vr.right <= vr.left || vr.bottom <= vr.top { return [0, 0, 0, 0]; }
        [vr.left - wr.left, vr.top - wr.top, wr.right - vr.right, wr.bottom - vr.bottom]
    }
}

/// 窗口的可视矩形 [x, y, w, h]（DWM 失败时退回 GetWindowRect）。
pub fn visible_rect(hwnd: i64) -> [i32; 4] {
    unsafe {
        let h = hwnd as HWND;
        let mut vr: RECT = Default::default();
        let ok = DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut vr as *mut _ as *mut c_void, std::mem::size_of::<RECT>() as u32);
        if ok == 0 && vr.right > vr.left && vr.bottom > vr.top {
            return [vr.left, vr.top, vr.right - vr.left, vr.bottom - vr.top];
        }
        let mut rc: RECT = Default::default();
        GetWindowRect(h, &mut rc);
        [rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top]
    }
}

/// 把窗口摆到目标矩形；连续调用时后摆放者在 Z 序上方（用于多层重叠）。
/// 摆位后做帧补偿：把隐形缩放边框藏到目标矩形外侧，让「可视边界」精确铺满目标矩形。
pub fn place(hwnd: i64, rect: [i32; 4], activate: bool) {
    unsafe {
        let h = hwnd as HWND;
        if IsWindow(h) == 0 { return; }
        if IsIconic(h) != 0 { ShowWindow(h, SW_RESTORE); }
        else if IsZoomed(h) != 0 { ShowWindow(h, SW_RESTORE); }
        let flags = if activate { 0u32 } else { SWP_NOACTIVATE };
        SetWindowPos(h, HWND_TOP, rect[0], rect[1], rect[2].max(120), rect[3].max(90), flags);
        let ins = frame_inset(hwnd);
        if ins != [0, 0, 0, 0] {
            SetWindowPos(h, HWND_TOP,
                rect[0] - ins[0], rect[1] - ins[1],
                (rect[2] + ins[0] + ins[2]).max(120), (rect[3] + ins[1] + ins[3]).max(90), flags);
        }
    }
}

pub fn launch(layer: &Layer) -> bool {
    if layer.cmd.trim().is_empty() { return false; }
    unsafe {
        let op = wide("open");
        let file = wide(&layer.cmd);
        let args = wide(&layer.args);
        let dir = wide(&layer.workdir);
        let r = ShellExecuteW(0, op.as_ptr(), file.as_ptr(),
            if layer.args.is_empty() { std::ptr::null() } else { args.as_ptr() },
            if layer.workdir.is_empty() { std::ptr::null() } else { dir.as_ptr() },
            SW_SHOWNORMAL);
        // ShellExecuteW 成功返回 > 32 的 HINSTANCE
        r as i64 > 32
    }
}

/// 窗口匹配：进程名 / 类名 / 标题，返回匹配分数（越大越可信）。
pub fn match_score(layer: &Layer, w: &RawWin) -> i32 {
    let mut score = 0;
    let mut specified = false;
    if !layer.process.is_empty() {
        specified = true;
        if w.process.eq_ignore_ascii_case(layer.process.trim()) { score += 10; }
        else if layer.cmd.rsplit(['\\', '/']).next().unwrap_or("").eq_ignore_ascii_case(&w.process) { score += 8; }
        else { return -1; }
    }
    if !layer.class.is_empty() {
        specified = true;
        if w.class.eq_ignore_ascii_case(layer.class.trim()) { score += 6; } else { return -1; }
    }
    if !layer.title.is_empty() {
        specified = true;
        if w.title.to_lowercase().contains(&layer.title.to_lowercase()) { score += 4; } else { return -1; }
    }
    if !specified {
        let exe = layer.cmd.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
        if !exe.is_empty() && w.process.to_lowercase() == exe { score += 10; }
        else { return -1; }
    }
    score
}

pub fn find_matching(layer: &Layer, exclude: &[i64]) -> Option<RawWin> {
    let mut best: Option<(i32, RawWin)> = None;
    for w in enum_windows() {
        if exclude.contains(&w.hwnd) { continue; }
        let s = match_score(layer, &w);
        if s > 0 && best.as_ref().map(|b| s > b.0).unwrap_or(true) { best = Some((s, w)); }
    }
    best.map(|b| b.1)
}

pub fn wait_for_window(layer: &Layer, exclude: &[i64], timeout_ms: u32) -> Option<RawWin> {
    let start = unsafe { GetTickCount64() };
    loop {
        if let Some(w) = find_matching(layer, exclude) { return Some(w); }
        let now = unsafe { GetTickCount64() };
        if now.saturating_sub(start) >= timeout_ms as u64 { return None; }
        unsafe { Sleep(120); }
    }
}

/// 目标窗口挑选：精确匹配红蜘蛛类窗口 -> 无边框且覆盖大部分工作区 -> 前台窗口。
pub fn pick_target(locked: i64) -> Option<RawWin> {
    if locked != 0 {
        if unsafe { IsWindow(locked as HWND) } != 0 { return win_by_hwnd(locked); }
    }
    let wins = enum_windows();
    for w in &wins {
        if w.class == "RedEagle.Monitor" || w.process.eq_ignore_ascii_case("REDAgent.exe") { return Some(w.clone()); }
    }
    let wa = primary_work_area();
    let mut best: Option<RawWin> = None;
    for w in &wins {
        let borderless = (w.style & WS_OVERLAPPEDWINDOW) != WS_OVERLAPPEDWINDOW || (w.style & WS_POPUP) != 0;
        if borderless && w.rect[2] >= (wa.w as f64 * 0.8) as i32 && w.rect[3] >= (wa.h as f64 * 0.7) as i32 {
            if best.as_ref().map(|b| w.rect[2] * w.rect[3] > b.rect[2] * b.rect[3]).unwrap_or(true) {
                best = Some(w.clone());
            }
        }
    }
    if best.is_some() { return best; }
    let fg = unsafe { GetForegroundWindow() } as i64;
    wins.into_iter().find(|w| w.hwnd == fg)
}

pub fn side_rect(side: &str, wa: &RectPx) -> [i32; 4] {
    let (x, y, w, h) = (wa.x, wa.y, wa.w, wa.h);
    match side {
        "left" => [x, y, w / 2, h],
        "right" => [x + w / 2, y, w - w / 2, h],
        "top" => [x, y, w, h / 2],
        "bottom" => [x, y + h / 2, w, h - h / 2],
        "q1" => [x, y, w / 2, h / 2],
        "q2" => [x + w / 2, y, w - w / 2, h / 2],
        "q3" => [x, y + h / 2, w / 2, h - h / 2],
        "q4" => [x + w / 2, y + h / 2, w - w / 2, h - h / 2],
        _ => [x, y, w, h],
    }
}

pub fn setup_dpi() {
    unsafe {
        // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
        if SetProcessDpiAwarenessContext(-4) == 0 {
            // 老系统回退：shcore::SetProcessDpiAwareness(2)，这里用 user32 的进程级 API
            SetProcessDpiAwarenessContext(-3);
        }
    }
}
