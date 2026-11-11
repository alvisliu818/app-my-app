//! 系统线程：托盘、全局热键（可自定义/可留空/可总关）、低级鼠标钩子（贴边）、守护定时器。
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::engine::{self, SharedRef};
use crate::ffi::*;
use crate::hotkeys;
use crate::model::*;

struct SysCtx {
    shared: SharedRef,
    app: AppHandle,
    hwnd: HWND,
    hook: HHOOK,
    registered: Vec<i32>,
    drag: Option<(i64, [i32; 4], POINT)>,
    drag_moved: bool,
}

static SYS_PTR: AtomicIsize = AtomicIsize::new(0);
fn ctx() -> &'static mut SysCtx {
    unsafe { &mut *(SYS_PTR.load(Ordering::Relaxed) as *mut SysCtx) }
}
fn has_ctx() -> bool { SYS_PTR.load(Ordering::Relaxed) != 0 }

pub fn start(shared: SharedRef, app: AppHandle) {
    std::thread::spawn(move || run(shared, app));
}

fn run(shared: SharedRef, app: AppHandle) {
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let cls = wide("WinLayoutSys");
        let title = wide("WinLayoutSys");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: sys_wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, cls.as_ptr(), title.as_ptr(), 0,
            0, 0, 0, 0, HWND_MESSAGE, 0, inst, 0);
        if hwnd == 0 {
            log_line(&shared.lock().unwrap().base, "系统窗口创建失败：托盘/热键不可用");
            return;
        }
        let boxed = Box::new(SysCtx {
            shared: shared.clone(), app: app.clone(), hwnd, hook: 0,
            registered: Vec::new(), drag: None, drag_moved: false,
        });
        SYS_PTR.store(Box::into_raw(boxed) as isize, Ordering::SeqCst);
        {
            let mut s = shared.lock().unwrap();
            s.sys_hwnd = hwnd as i64;
            s.log("系统线程已启动（托盘 / 热键 / 贴边 / 守护）");
        }
        refresh();      // 同时完成热键注册、钩子安装、定时器设置
        add_tray(hwnd);

        let mut msg: MSG = Default::default();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        cleanup();
    }
}

fn unregister_all(c: &mut SysCtx) {
    for id in c.registered.drain(..) {
        unsafe { UnregisterHotKey(c.hwnd, id); }
    }
}

/// 按当前设置重新注册全部热键：总开关、留空、自定义组合都在这里生效。
fn register_from_settings(c: &mut SysCtx) {
    unregister_all(c);
    let (base, settings) = {
        let s = c.shared.lock().unwrap();
        (s.base.clone(), s.store.settings.clone())
    };
    if !settings.hotkeys_enabled {
        c.shared.lock().unwrap().hotkey_effective.clear();
        log_line(&base, "快捷键总开关已关闭：未注册任何热键");
        return;
    }
    let mut effective: HashMap<String, String> = HashMap::new();
    let (mut ok, mut fail, mut skipped) = (0, 0, 0);
    for (i, a) in hotkeys::ACTIONS.iter().enumerate() {
        let key = hotkeys::effective_key(&settings, a.id);
        if key.is_empty() { skipped += 1; continue; }
        let (mods, vk) = match hotkeys::parse(&key) {
            Ok(v) => v,
            Err(e) => {
                fail += 1;
                log_line(&base, &format!("热键 {} 配置无效「{}」：{}", a.id, key, e));
                continue;
            }
        };
        let id = HOTKEY_ID_BASE + i as i32;
        // 一律按设定值注册，不做任何自动回退；失败就如实记为被占用，由用户改键或留空
        let mut good = unsafe { RegisterHotKey(c.hwnd, id, mods | MOD_NOREPEAT, vk) } != 0;
        if !good { good = unsafe { RegisterHotKey(c.hwnd, id, mods, vk) } != 0; }
        if good {
            c.registered.push(id);
            ok += 1;
            effective.insert(a.id.to_string(), key.clone());
        } else {
            fail += 1;
            log_line(&base, &format!("热键注册失败 {} = {}（不自动回退；点「检测占用」查是谁占的，或改键/留空）", a.id, key));
        }
    }
    c.shared.lock().unwrap().hotkey_effective = effective;
    log_line(&base, &format!("注册热键 成功={} 失败={} 留空={}（不自动回退）", ok, fail, skipped));
}

fn refresh() {
    let c = ctx();
    let st = c.shared.lock().unwrap().store.settings.clone();
    unsafe {
        if st.snap_enabled && c.hook == 0 {
            c.hook = SetWindowsHookExW(WH_MOUSE_LL, mouse_hook, 0, 0);
            if c.hook == 0 { log_line(&c.shared.lock().unwrap().base, "鼠标钩子安装失败"); }
        } else if !st.snap_enabled && c.hook != 0 {
            UnhookWindowsHookEx(c.hook);
            c.hook = 0;
        }
        KillTimer(c.hwnd, TIMER_GUARD);
        SetTimer(c.hwnd, TIMER_GUARD, st.guard_ms.max(100), 0);
    }
    register_from_settings(&mut *c);
}

/// 检测占用：先注销自己的热键（否则会「被自己占用」），探测后恢复注册。
fn run_detection(seq: u64) {
    let c = ctx();
    unregister_all(&mut *c);
    let settings = c.shared.lock().unwrap().store.settings.clone();
    let mut report = hotkeys::detect(&settings, seq);
    {
        let mut s = c.shared.lock().unwrap();
        if let Some(prev) = s.last_detect.clone() {
            hotkeys::infer_owners(&prev, &mut report);
        }
        s.last_detect = Some(report.clone());
        s.detect_result = Some((seq, report.clone()));
        s.log(&format!("快捷键检测完成：可用 {} / 被占用 {} / 留空 {}",
            report.ok_count, report.occupied_count, report.empty_count));
    }
    register_from_settings(&mut *c);
}

fn add_tray(hwnd: HWND) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = Default::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_MSG;
        nid.hIcon = LoadIconW(0, IDI_APPLICATION);
        let tip = wide("WinLayout · 窗口布局管家");
        let n = tip.len().min(128);
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        Shell_NotifyIconW(NIM_ADD, &mut nid);
    }
}

fn cleanup() {
    let c = ctx();
    unsafe {
        if c.hook != 0 { UnhookWindowsHookEx(c.hook); c.hook = 0; }
        KillTimer(c.hwnd, TIMER_GUARD);
        unregister_all(&mut *c);
        let mut nid: NOTIFYICONDATAW = Default::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = c.hwnd;
        nid.uID = 1;
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}

unsafe extern "system" fn sys_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if !has_ctx() { return DefWindowProcW(hwnd, msg, wp, lp); }
    match msg {
        WM_HOTKEY => handle_hotkey(wp as i32),
        TRAY_MSG => {
            let mouse = low_word(lp) as u32;
            if mouse == WM_RBUTTONUP || mouse == WM_LBUTTONDBLCLK { handle_tray(mouse); }
        }
        WM_TIMER => engine::guard_tick(&ctx().shared),
        WM_REFRESH => refresh(),
        WM_DETECT => run_detection(wp as u64),
        WM_DESTROY => PostQuitMessage(0),
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn quick_capture(shared: &SharedRef) {
    let name = format!("布局 {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    match engine::capture_current(shared, &name, &[], true) {
        Ok(_) => { shared.lock().unwrap().log(&format!("托盘：已保存当前布局「{}」", name)); }
        Err(e) => { shared.lock().unwrap().log(&format!("托盘保存失败: {}", e)); }
    }
}

fn handle_hotkey(id: i32) {
    let idx = id - HOTKEY_ID_BASE;
    if idx < 0 || idx as usize >= hotkeys::ACTIONS.len() { return; }
    let action = hotkeys::ACTIONS[idx as usize].id;
    let shared = ctx().shared.clone();
    let mut note: Option<String> = None;
    if let Some(n) = action.strip_prefix("layout_").and_then(|s| s.parse::<usize>().ok()) {
        let name = shared.lock().unwrap().store.layouts.get(n.saturating_sub(1)).map(|l| l.name.clone());
        if let Some(nm) = name {
            match engine::apply_layout(&shared, &nm) {
                Ok(r) => note = Some(format!("已应用布局「{}」· {}", nm, r.lines().next().unwrap_or(""))),
                Err(e) => note = Some(format!("应用布局失败: {}", e)),
            }
        } else {
            note = Some(format!("没有第 {} 个布局", n));
        }
    } else {
        let side = match action {
            "arrange_left" | "legacy_left" => Some("left"),
            "arrange_right" | "legacy_right" => Some("right"),
            "arrange_full" | "legacy_full" => Some("full"),
            _ => None,
        };
        if let Some(s) = side {
            if let Ok(r) = engine::arrange(&shared, s, None) { note = Some(r); }
        } else {
            match action {
                "arrange_restore" | "legacy_restore" | "restore_original" => {
                    let _ = engine::restore_original(&shared, None);
                    note = Some("已还原原状".into());
                }
                "lock_foreground" => note = Some(engine::lock_foreground(&shared)),
                "toggle_guard" => {
                    let mut s = shared.lock().unwrap();
                    s.guard_on = !s.guard_on;
                    note = Some(if s.guard_on { "保持状态：开" } else { "保持状态：关" }.into());
                }
                "window_info" => note = Some(engine::window_info(None, &shared)),
                "save_current" => { quick_capture(&shared); note = Some("已保存当前布局".into()); }
                _ => {}
            }
        }
    }
    if let Some(n) = note {
        let s = shared.lock().unwrap();
        s.log(&format!("[热键 {}] {}", action, n.replace('\n', " | ")));
    }
}

fn handle_tray(mouse: u32) {
    let c = ctx();
    if mouse == WM_LBUTTONDBLCLK { show_main(&c.app); return; }
    let (layouts, autostart, guard, base) = {
        let s = c.shared.lock().unwrap();
        (s.store.layouts.clone(), autostart_get(), s.guard_on, s.base.clone())
    };
    unsafe {
        let menu = CreatePopupMenu();
        let mut texts: Vec<Vec<u16>> = Vec::new();
        let mut add = |text: String, id: u32, checked: bool| {
            let t = wide(&text);
            texts.push(t);
            let p = texts.last().unwrap().as_ptr();
            let flags = MF_STRING | (if checked { MF_CHECKED } else { MF_UNCHECKED });
            AppendMenuW(menu, flags, id as usize, p);
        };
        add("显示主窗口".into(), ID_SHOW, false);
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        for (i, l) in layouts.iter().take(9).enumerate() {
            add(format!("应用布局：{}", l.name), ID_LAYOUT_BASE + i as u32, false);
        }
        add("★ 保存当前布局".into(), ID_CAPTURE, false);
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        add("左半屏".into(), ID_ARR_LEFT, false);
        add("右半屏".into(), ID_ARR_RIGHT, false);
        add("铺满工作区".into(), ID_ARR_FULL, false);
        add("还原原状".into(), ID_RESTORE, false);
        add("窗口信息".into(), ID_INFO, false);
        add("快捷键设置 / 检测占用…".into(), ID_HOTKEYS, false);
        add("保持状态".into(), ID_GUARD, guard);
        add("开机自启".into(), ID_AUTOSTART, autostart);
        add("打开日志".into(), ID_LOG, false);
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        add("退出".into(), ID_QUIT, false);

        let mut pt: POINT = Default::default();
        GetCursorPos(&mut pt);
        SetForegroundWindow(c.hwnd);
        let cmd = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, c.hwnd, std::ptr::null_mut());
        PostMessageW(c.hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        let shared = c.shared.clone();
        match cmd as u32 {
            ID_SHOW => show_main(&c.app),
            ID_HOTKEYS => {
                show_main(&c.app);
                let _ = c.app.emit("open-hotkeys", "tray");
            }
            ID_CAPTURE => quick_capture(&c.shared),
            ID_ARR_LEFT => { let _ = engine::arrange(&shared, "left", None); }
            ID_ARR_RIGHT => { let _ = engine::arrange(&shared, "right", None); }
            ID_ARR_FULL => { let _ = engine::arrange(&shared, "full", None); }
            ID_RESTORE => { let _ = engine::restore_original(&shared, None); }
            ID_INFO => { let t = engine::window_info(None, &shared); shared.lock().unwrap().log(&t.replace('\n', " | ")); }
            ID_GUARD => { let mut s = shared.lock().unwrap(); s.guard_on = !s.guard_on; }
            ID_AUTOSTART => {
                let v = !autostart_get();
                autostart_set(v);
                let mut s = shared.lock().unwrap();
                s.store.settings.auto_start = v;
                s.persist();
            }
            ID_LOG => {
                let op = wide("open");
                let p = wide(&log_path(&base).to_string_lossy());
                ShellExecuteW(0, op.as_ptr(), p.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
            }
            ID_QUIT => std::process::exit(0),
            id if id >= ID_LAYOUT_BASE && id < ID_LAYOUT_BASE + layouts.len() as u32 => {
                let l = &layouts[(id - ID_LAYOUT_BASE) as usize];
                let _ = engine::apply_layout(&shared, &l.name);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION && has_ctx() {
        let c = ctx();
        let ms = &*(lp as *const MSLLHOOKSTRUCT);
        if wp == WM_LBUTTONDOWN as usize {
            let h = GetAncestor(WindowFromPoint(ms.pt), GA_ROOT) as i64;
            if h != 0 && IsWindow(h as HWND) != 0 {
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(h as HWND, &mut pid);
                if pid != GetCurrentProcessId() && IsWindowVisible(h as HWND) != 0 {
                    let mut rc: RECT = Default::default();
                    GetWindowRect(h as HWND, &mut rc);
                    c.drag = Some((h, [rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top], ms.pt));
                    c.drag_moved = false;
                }
            }
        } else if wp == WM_MOUSEMOVE as usize {
            if let Some((_, _, start)) = c.drag {
                if (ms.pt.x - start.x).abs() + (ms.pt.y - start.y).abs() > 20 { c.drag_moved = true; }
            }
        } else if wp == WM_LBUTTONUP as usize {
            if let Some((h, _, _)) = c.drag.take() {
                let moved = c.drag_moved;
                c.drag_moved = false;
                if moved && IsWindow(h as HWND) != 0 {
                    let mut rc: RECT = Default::default();
                    GetWindowRect(h as HWND, &mut rc);
                    let cur = [rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top];
                    let ctrl = (GetAsyncKeyState(VK_CONTROL) as u16 & 0x8000) != 0;
                    engine::snap_from_drag(&c.shared, h, cur, ctrl);
                }
            }
        }
        let hhk = c.hook;
        return CallNextHookEx(hhk, code, wp, lp);
    }
    unsafe { CallNextHookEx(0, code, wp, lp) }
}
