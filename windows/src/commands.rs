//! Tauri 命令：前端通过 invoke 调用。
use tauri::State;

use crate::engine::{self, SharedRef};
use crate::ffi::*;
use crate::model::*;
use crate::win;

#[tauri::command]
pub fn get_state(shared: State<'_, SharedRef>) -> UiState {
    let ui = engine::ui_state(shared.inner());
    shared.inner().lock().unwrap().log("IPC get_state（前端已连接）");
    ui
}

#[tauri::command]
pub fn save_state(settings: Settings, layouts: Vec<Layout>, shared: State<'_, SharedRef>) -> Result<(), String> {
    let mut s = shared.inner().lock().unwrap();
    s.store.settings = settings;
    s.store.layouts = layouts;
    s.persist();
    Ok(())
}

#[tauri::command]
pub fn update_settings(settings: Settings, shared: State<'_, SharedRef>) -> Result<(), String> {
    let arc = shared.inner().clone();
    let mut s = arc.lock().unwrap();
    if autostart_get() != settings.auto_start { autostart_set(settings.auto_start); }
    s.store.settings = settings;
    s.persist();
    s.notify_sys(); // 让系统线程重装/卸载钩子与定时器
    s.log("设置已更新");
    Ok(())
}

#[tauri::command]
pub fn set_active_layout(name: String, shared: State<'_, SharedRef>) {
    shared.inner().lock().unwrap().active_layout = Some(name);
}

#[tauri::command]
pub fn list_windows() -> Vec<WinInfo> { win::windows_json() }

/// 耗时命令用 async，避免阻塞 WebView 所在线程（应用布局可能等待程序启动）。
#[tauri::command]
pub async fn apply_layout(name: String, shared: State<'_, SharedRef>) -> Result<String, String> {
    let arc = shared.inner().clone();
    engine::apply_layout(&arc, &name)
}

#[tauri::command]
pub async fn capture_current(name: String, hwnds: Vec<i64>, group_overlap: bool,
    shared: State<'_, SharedRef>) -> Result<UiState, String> {
    let arc = shared.inner().clone();
    engine::capture_current(&arc, &name, &hwnds, group_overlap)
}

#[tauri::command]
pub fn arrange(side: String, hwnd: Option<i64>, shared: State<'_, SharedRef>) -> Result<String, String> {
    engine::arrange(shared.inner(), &side, hwnd)
}

#[tauri::command]
pub fn restore_original(hwnd: Option<i64>, shared: State<'_, SharedRef>) -> Result<String, String> {
    engine::restore_original(shared.inner(), hwnd)
}

#[tauri::command]
pub fn lock_foreground(shared: State<'_, SharedRef>) -> String {
    engine::lock_foreground(shared.inner())
}

#[tauri::command]
pub fn window_info(hwnd: Option<i64>, shared: State<'_, SharedRef>) -> String {
    engine::window_info(hwnd, shared.inner())
}

#[tauri::command]
pub fn selftest(shared: State<'_, SharedRef>) -> String {
    engine::selftest(shared.inner())
}

#[tauri::command]
pub fn open_log(shared: State<'_, SharedRef>) {
    let base = shared.inner().lock().unwrap().base.clone();
    let p = log_path(&base);
    unsafe {
        let op = wide("open");
        let path = wide(&p.to_string_lossy());
        ShellExecuteW(0, op.as_ptr(), path.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

/// 快捷键占用检测：交给系统线程执行（先注销自身热键，避免「被自己占用」），轮询取回结果。
#[tauri::command]
pub async fn detect_hotkeys(shared: State<'_, SharedRef>) -> Result<crate::hotkeys::DetectReport, String> {
    let arc = shared.inner().clone();
    let seq = {
        let mut s = arc.lock().unwrap();
        if s.sys_hwnd == 0 { return Err("系统线程尚未启动".into()); }
        s.detect_seq += 1;
        s.detect_result = None;
        let seq = s.detect_seq;
        unsafe { PostMessageW(s.sys_hwnd as HWND, WM_DETECT, seq as WPARAM, 0); }
        seq
    };
    for _ in 0..160 { // 最多等 8 秒
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut s = arc.lock().unwrap();
        if let Some((got, report)) = s.detect_result.take() {
            if got == seq { return Ok(report); }
            s.detect_result = Some((got, report));
        }
    }
    Err("检测超时：系统线程未响应".into())
}

/// 检测辅助：由用户主动点击触发，关闭/结束某个可疑进程以便「重新检测」做差异推断。
#[tauri::command]
pub fn probe_stop_process(pid: u32, force: bool, shared: State<'_, SharedRef>) -> Result<String, String> {
    let msg = crate::hotkeys::stop_process(pid, force)?;
    shared.inner().lock().unwrap().log(&format!("快捷键检测辅助：{}", msg));
    Ok(msg)
}

#[tauri::command]
pub fn frontend_log(msg: String, shared: State<'_, SharedRef>) {
    shared.inner().lock().unwrap().log(&format!("[UI] {}", msg));
}

#[tauri::command]
pub fn quit() { std::process::exit(0); }
