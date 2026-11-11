#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod engine;
mod ffi;
mod hotkeys;
mod model;
mod sys;
mod win;

use std::sync::{Arc, Mutex};

use engine::{Shared, SharedRef};
use model::*;

fn main() {
    win::setup_dpi();
    let base = exe_dir();
    let store = load_store(&base);
    let shared: SharedRef = Arc::new(Mutex::new(Shared::new(base.clone(), store)));

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        unsafe { ffi::AttachConsole(0xFFFF_FFFF); } // ATTACH_PARENT_PROCESS，便于命令行看到输出
        let out = cli(&args, &shared, &base);
        cli_output(&base, &out);
        return;
    }

    // GUI 模式：不要控制台窗口
    unsafe { ffi::FreeConsole(); }

    let managed = shared.clone();
    tauri::Builder::default()
        .manage(managed.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            {
                let mut s = managed.lock().unwrap();
                s.app = Some(handle.clone());
                log_line(&s.base, &format!("启动 GUI，exe={:?}", exe_dir()));
            }
            sys::start(managed.clone(), handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide(); // 关闭 = 最小化到托盘
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_state,
            commands::update_settings,
            commands::set_active_layout,
            commands::list_windows,
            commands::apply_layout,
            commands::capture_current,
            commands::arrange,
            commands::restore_original,
            commands::lock_foreground,
            commands::window_info,
            commands::selftest,
            commands::open_log,
            commands::frontend_log,
            commands::detect_hotkeys,
            commands::probe_stop_process,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("WinLayout 启动失败");
}

/// 命令行单次模式（保留 WindowHalf 原有的 --left/--right/--full/--restore）。
fn cli(args: &[String], shared: &SharedRef, _base: &std::path::Path) -> String {
    let op = args[0].as_str();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match op {
        "--left" | "--right" | "--full" | "--restore" => {
            let side = &op[2..];
            if side == "restore" {
                engine::restore_original(shared, None).unwrap_or_else(|e| e)
            } else {
                engine::arrange(shared, side, None).unwrap_or_else(|e| e)
            }
        }
        "--apply" => {
            let name = arg(1);
            if name.is_empty() { return "用法: winlayout.exe --apply <布局名>".into(); }
            engine::apply_layout(shared, &name).unwrap_or_else(|e| e)
        }
        "--capture" => {
            let name = if arg(1).is_empty() {
                format!("布局 {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
            } else { arg(1) };
            match engine::capture_current(shared, &name, &[], true) {
                Ok(st) => format!("已保存当前布局「{}」，共 {} 个分区\n布局列表:\n{}",
                    name,
                    st.layouts.last().map(|l| l.tiles.len()).unwrap_or(0),
                    st.layouts.iter().map(|l| format!("  - {}", l.name)).collect::<Vec<_>>().join("\n")),
                Err(e) => e,
            }
        }
        "--list" => {
            let s = shared.lock().unwrap();
            let mut out = format!("共 {} 个布局:\n", s.store.layouts.len());
            for (i, l) in s.store.layouts.iter().enumerate() {
                let n: usize = l.tiles.iter().map(|t| t.layers.len()).sum();
                out.push_str(&format!("  {}. {}   {} 分区 / {} 应用\n", i + 1, l.name, l.tiles.len(), n));
            }
            out
        }
        "--selftest" => engine::selftest(shared),
        "--hotkeys" => {
            let s = shared.lock().unwrap().store.settings.clone();
            let r = hotkeys::detect(&s, 0);
            hotkeys::format_report(&r)
        }
        "--stop-probe" => {
            let pid = arg(1).parse::<u32>().unwrap_or(0);
            if pid == 0 { "用法: winlayout.exe --stop-probe <PID> [--force]".into() }
            else {
                let force = args.iter().any(|a| a == "--force");
                hotkeys::stop_process(pid, force).unwrap_or_else(|e| e)
            }
        }
        "--demo" => engine::demo(shared),
        "--info" => engine::window_info(None, shared),
        "--lock" => engine::lock_foreground(shared),
        _ => format!("未知参数: {}\n用法:\n  winlayout.exe --left|--right|--full|--restore\n  winlayout.exe --apply <布局名>\n  winlayout.exe --capture [布局名]\n  winlayout.exe --list | --hotkeys | --selftest | --demo | --info | --lock\n  winlayout.exe --stop-probe <PID> [--force]    :: 检测辅助：关闭/结束可疑进程", op),
    }
}
