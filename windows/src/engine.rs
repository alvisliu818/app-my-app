//! 布局引擎：应用布局、保存当前布局、快速分屏、还原、贴边、守护、自检。
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};

use crate::ffi::*;
use crate::model::*;
use crate::win;

pub type SharedRef = Arc<Mutex<Shared>>;

pub struct Shared {
    pub base: PathBuf,
    pub store: Store,
    pub locked_hwnd: i64,
    pub placements: Vec<Placement>,
    pub guard_on: bool,
    pub active_layout: Option<String>,
    pub sys_hwnd: i64,
    pub app: Option<AppHandle>,
    /// 动作 id -> 实际注册成功的组合键（空 = 未注册）
    pub hotkey_effective: HashMap<String, String>,
    /// 检测请求序号与结果（系统线程写回）
    pub detect_seq: u64,
    pub detect_result: Option<(u64, crate::hotkeys::DetectReport)>,
    pub last_detect: Option<crate::hotkeys::DetectReport>,
}

impl Shared {
    pub fn new(base: PathBuf, store: Store) -> Self {
        Self {
            base, store, locked_hwnd: 0, placements: Vec::new(), guard_on: false,
            active_layout: None, sys_hwnd: 0, app: None,
            hotkey_effective: HashMap::new(), detect_seq: 0,
            detect_result: None, last_detect: None,
        }
    }
    pub fn emit(&self, event: &str, text: String) {
        if let Some(app) = &self.app { let _ = app.emit(event, text); }
    }
    pub fn log(&self, msg: &str) {
        log_line(&self.base, msg);
        self.emit("winlayout-log", msg.to_string());
    }
    pub fn persist(&mut self) { save_store(&self.base, &self.store); }
    pub fn notify_sys(&self) {
        if self.sys_hwnd != 0 { unsafe { PostMessageW(self.sys_hwnd as HWND, WM_REFRESH, 0, 0); } }
    }
}

pub fn ui_state(shared: &SharedRef) -> UiState {
    let s = shared.lock().unwrap();
    UiState {
        settings: s.store.settings.clone(),
        layouts: s.store.layouts.clone(),
        work_area: win::primary_work_area(),
        monitors: win::all_monitors(),
        autostart: autostart_get(),
        hotkey_actions: crate::hotkeys::actions_state(&s.store.settings, &s.hotkey_effective),
        auto_detect: std::env::var_os("WINLAYOUT_AUTO_DETECT").is_some(),
    }
}

/// 应用布局：打开对应 app（未运行才启动）→ 等窗口 → 修样式 → 按矩形与 Z 序摆放。
/// 同一分区多个 layer 按数组顺序从底到顶叠放，并带层叠偏移。
pub fn apply_layout(shared: &SharedRef, name: &str) -> Result<String, String> {
    let (layout, settings, base) = {
        let s = shared.lock().unwrap();
        let l = s.store.layouts.iter().find(|l| l.name == name).cloned()
            .ok_or_else(|| format!("找不到布局「{}」", name))?;
        (l, s.store.settings.clone(), s.base.clone())
    };
    let wa = win::primary_work_area();
    let mut used: Vec<i64> = Vec::new();
    let mut placements: Vec<Placement> = Vec::new();
    let mut lines: Vec<String> = Vec::new();

    for (ti, tile) in layout.tiles.iter().enumerate() {
        let base_rect = win::rect_to_px(tile.rect, &wa);
        for (li, layer) in tile.layers.iter().enumerate() {
            let existing = win::find_matching(layer, &used);
            let found = if existing.is_some() && layer.only_if_missing {
                existing.clone()
            } else {
                if !layer.cmd.trim().is_empty() {
                    if win::launch(layer) {
                        log_line(&base, &format!("启动 {} {}", layer.cmd, layer.args));
                    } else {
                        lines.push(format!("启动失败: {}", layer.cmd));
                    }
                }
                win::wait_for_window(layer, &used, settings.wait_ms)
            };
            let w = match found.or(existing) {
                Some(w) => w,
                None => {
                    lines.push(format!("分区{} 第{}层「{}」未找到窗口", ti + 1, li + 1,
                        if layer.name.is_empty() { &layer.cmd } else { &layer.name }));
                    continue;
                }
            };
            if layer.fix_style && settings.fix_style {
                let mut s = shared.lock().unwrap();
                let base = s.base.clone();
                win::fix_style(w.hwnd, &base, &mut s.store);
                s.persist();
            }
            // 顶层偏移最大（与前端卡片绘制一致）
            let off_x = li as i32 * tile.cascade[0].clamp(0, 200);
            let off_y = li as i32 * tile.cascade[1].clamp(0, 200);
            let r = [base_rect[0] + off_x, base_rect[1] + off_y, base_rect[2], base_rect[3]];
            win::place(w.hwnd, r, false);
            // 偏差按「可视边界」核对（place 做了帧补偿，外矩形会比目标大出一圈边框）
            let actual = win::visible_rect(w.hwnd);
            let ok = (0..4).all(|i| (actual[i] - r[i]).abs() <= 2);
            used.push(w.hwnd);
            placements.push(Placement { hwnd: w.hwnd, rect: r });
            lines.push(format!("分区{}·第{}层 {} → [{},{} {}×{}] 实际 [{},{} {}×{}] {}",
                ti + 1, li + 1,
                if layer.name.is_empty() { &layer.process } else { &layer.name },
                r[0], r[1], r[2], r[3], actual[0], actual[1], actual[2], actual[3],
                if ok { "✓" } else { "(有偏差)" }));
        }
    }
    {
        let mut s = shared.lock().unwrap();
        s.placements = placements;
        s.guard_on = settings.guard_enabled;
        s.active_layout = Some(name.to_string());
        s.log(&format!("应用布局「{}」：已摆放 {} 个窗口", name, used.len()));
    }
    Ok(format!("已摆放 {} 个窗口\n{}", used.len(), lines.join("\n")))
}

fn overlap_ratio(a: [f64; 4], b: [f64; 4]) -> f64 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    if x2 <= x1 || y2 <= y1 { return 0.0; }
    let inter = (x2 - x1) * (y2 - y1);
    let min_area = (a[2] * a[3]).min(b[2] * b[3]);
    if min_area <= 0.0 { 0.0 } else { inter / min_area }
}

fn union_rect(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let r = (a[0] + a[2]).max(b[0] + b[2]);
    let bottom = (a[1] + a[3]).max(b[1] + b[3]);
    [x, y, r - x, bottom - y]
}

/// 保存当前布局：把桌面上现有窗口的位置、启动命令、Z 序固化为一个布局。
/// 重叠窗口会合并为同一分区的多层（layers 从底到顶）。
pub fn capture_current(shared: &SharedRef, name: &str, hwnds: &[i64], group_overlap: bool) -> Result<UiState, String> {
    let wa = win::primary_work_area();
    let mut wins: Vec<win::RawWin> = win::enum_windows()
        .into_iter()
        .filter(|w| hwnds.is_empty() || hwnds.contains(&w.hwnd))
        .collect();
    // EnumWindows 由顶到底，反转为底 -> 顶，与 layers 顺序一致
    wins.reverse();

    let mut tiles: Vec<Tile> = Vec::new();
    for w in wins {
        let mut n = win::rect_to_norm(w.rect, &wa);
        n[0] = n[0].clamp(0.0, 1.0);
        n[1] = n[1].clamp(0.0, 1.0);
        n[2] = n[2].clamp(0.03, 1.0 - n[0]);
        n[3] = n[3].clamp(0.03, 1.0 - n[1]);
        let layer = Layer {
            id: new_id(),
            name: w.title.chars().take(40).collect(),
            cmd: w.exe.clone(),
            process: w.process.clone(),
            class: w.class.clone(),
            title: String::new(),
            only_if_missing: true,
            fix_style: true,
            ..Default::default()
        };
        let mut merged = false;
        if group_overlap {
            if let Some(t) = tiles.iter_mut().rev().find(|t| overlap_ratio(t.rect, n) > 0.55) {
                let dx = ((n[0] - t.rect[0]) * wa.w as f64).round() as i32;
                let dy = ((n[1] - t.rect[1]) * wa.h as f64).round() as i32;
                if dx.abs() + dy.abs() > 4 {
                    t.cascade = [dx.abs().clamp(8, 120), dy.abs().clamp(8, 120)];
                }
                let u = union_rect(t.rect, n);
                t.layers.push(layer.clone());
                t.rect = u;
                merged = true;
            }
        }
        if !merged {
            tiles.push(Tile { id: new_id(), rect: n, cascade: [24, 24], layers: vec![layer] });
        }
    }
    if tiles.is_empty() { return Err("没有可保存的窗口".to_string()); }
    let layout = Layout { name: name.to_string(), tiles };
    {
        let mut s = shared.lock().unwrap();
        s.store.layouts.retain(|l| l.name != name);
        s.store.layouts.push(layout);
        s.active_layout = Some(name.to_string());
        s.persist();
        s.log(&format!("保存当前布局「{}」：{} 个分区", name, s.store.layouts.last().unwrap().tiles.len()));
        s.emit("layouts-changed", name.to_string());
    }
    Ok(ui_state(shared))
}

/// 快速分屏（WindowHalf 原有能力）：left/right/full/top/bottom/q1..q4。
pub fn arrange(shared: &SharedRef, side: &str, hwnd: Option<i64>) -> Result<String, String> {
    let (locked, settings) = {
        let s = shared.lock().unwrap();
        (s.locked_hwnd, s.store.settings.clone())
    };
    let target = match hwnd {
        Some(h) => win::win_by_hwnd(h),
        None => win::pick_target(locked),
    }.ok_or_else(|| "没有找到目标窗口".to_string())?;

    if settings.fix_style {
        let mut s = shared.lock().unwrap();
        let base = s.base.clone();
        win::fix_style(target.hwnd, &base, &mut s.store);
        s.persist();
    }
    let wa = win::work_area_for(target.hwnd);
    let r = win::side_rect(side, &wa);
    win::place(target.hwnd, r, false);
    {
        let mut s = shared.lock().unwrap();
        s.locked_hwnd = target.hwnd;
        s.placements = vec![Placement { hwnd: target.hwnd, rect: r }];
        s.guard_on = settings.guard_enabled;
        s.log(&format!("分屏 {} hwnd={} rect={:?}", side, target.hwnd, r));
    }
    Ok(format!("{} → [{},{} {}×{}]", side, r[0], r[1], r[2], r[3]))
}

pub fn restore_original(shared: &SharedRef, hwnd: Option<i64>) -> Result<String, String> {
    let (h, orig, base) = {
        let s = shared.lock().unwrap();
        let h = hwnd.unwrap_or(s.locked_hwnd);
        if h == 0 { return Err("没有锁定目标窗口".to_string()); }
        let o = s.store.originals.get(&h.to_string()).cloned()
            .ok_or_else(|| "没有记录该窗口的原始状态".to_string())?;
        (h, o, s.base.clone())
    };
    win::restore_style(h, &orig);
    {
        let mut s = shared.lock().unwrap();
        s.store.originals.remove(&h.to_string());
        s.persist();
        s.placements.clear();
        s.guard_on = false;
        s.log(&format!("还原原始样式与位置 hwnd={}", h));
    }
    let _ = base;
    Ok(format!("已还原 hwnd={} 的原始样式与位置", h))
}

pub fn lock_foreground(shared: &SharedRef) -> String {
    let h = unsafe { GetForegroundWindow() } as i64;
    if h == 0 { return "没有前台窗口".into(); }
    let info = win::win_by_hwnd(h);
    let mut s = shared.lock().unwrap();
    s.locked_hwnd = h;
    s.log(&format!("锁定前台窗口为目标 hwnd={} {:?}", h,
        info.map(|w| format!("{} / {}", w.process, w.class)).unwrap_or_default()));
    format!("已锁定 hwnd={}", h)
}

pub fn window_info(hwnd: Option<i64>, shared: &SharedRef) -> String {
    let h = hwnd.unwrap_or_else(|| shared.lock().unwrap().locked_hwnd);
    let w = win::win_by_hwnd(h).or_else(|| win::pick_target(0));
    match w {
        Some(w) => {
            let wa = win::work_area_for(w.hwnd);
            format!("类名: {}\n进程: {} (PID {})\nHWND: {}\nStyle: 0x{:X}\nExStyle: 0x{:X}\n窗口矩形: {}, {}  {}×{}\n所在显示器工作区: {}, {} ~ {}, {}",
                w.class, w.process, w.pid, w.hwnd, w.style, w.ex,
                w.rect[0], w.rect[1], w.rect[2], w.rect[3],
                wa.x, wa.y, wa.x + wa.w, wa.y + wa.h)
        }
        None => "没有找到目标窗口".into(),
    }
}

/// 实时贴边：拖动窗口松手时调用。Ctrl 拖动 -> 落入当前布局最近的分区。
pub fn snap_from_drag(shared: &SharedRef, hwnd: i64, cur: [i32; 4], ctrl: bool) {
    let (settings, layout) = {
        let s = shared.lock().unwrap();
        let active = s.active_layout.clone();
        let l = s.store.layouts.iter()
            .find(|l| Some(&l.name) == active.as_ref())
            .or_else(|| s.store.layouts.first()).cloned();
        (s.store.settings.clone(), l)
    };
    if !settings.snap_enabled { return; }
    let wa = win::work_area_containing(cur);
    let mut target: Option<[i32; 4]> = None;
    let mut reason = String::new();

    if ctrl {
        if let Some(l) = &layout {
            if !l.tiles.is_empty() {
                let cx = (cur[0] + cur[2] / 2) as f64;
                let cy = (cur[1] + cur[3] / 2) as f64;
                let mut best: Option<(f64, [f64; 4])> = None;
                for t in &l.tiles {
                    let r = win::rect_to_px(t.rect, &wa);
                    let d = ((cx - (r[0] + r[2] / 2) as f64).powi(2)
                        + (cy - (r[1] + r[3] / 2) as f64).powi(2)).sqrt();
                    if best.as_ref().map(|b| d < b.0).unwrap_or(true) { best = Some((d, t.rect)); }
                }
                if let Some((_, nr)) = best {
                    target = Some(win::rect_to_px(nr, &wa));
                    reason = "Ctrl 拖入分区".into();
                }
            }
        }
    } else {
        let th = settings.snap_px.max(2);
        let near_l = (cur[0] - wa.x).abs() <= th;
        let near_r = ((wa.x + wa.w) - (cur[0] + cur[2])).abs() <= th;
        let near_t = (cur[1] - wa.y).abs() <= th;
        let near_b = ((wa.y + wa.h) - (cur[1] + cur[3])).abs() <= th;
        let side = match (near_l, near_r, near_t, near_b) {
            (true, _, true, _) => Some("q1"),
            (_, true, true, _) => Some("q2"),
            (true, _, _, true) => Some("q3"),
            (_, true, _, true) => Some("q4"),
            (true, _, _, _) => Some("left"),
            (_, true, _, _) => Some("right"),
            (_, _, true, _) => Some("full"),
            (_, _, _, true) => Some("bottom"),
            _ => None,
        };
        if let Some(s) = side {
            target = Some(win::side_rect(s, &wa));
            reason = format!("贴边 {}", s);
        }
    }
    let Some(r) = target else { return };
    if settings.fix_style {
        let mut s = shared.lock().unwrap();
        let base = s.base.clone();
        win::fix_style(hwnd, &base, &mut s.store);
        s.persist();
    }
    win::place(hwnd, r, false);
    let mut s = shared.lock().unwrap();
    s.placements.retain(|p| p.hwnd != hwnd);
    s.placements.push(Placement { hwnd, rect: r });
    s.guard_on = settings.guard_enabled;
    s.log(&format!("{} hwnd={} rect={:?}", reason, hwnd, r));
}

/// 守护定时器：无边框/POPUP 样式被改回时自动修复（AHK 原有能力）。
/// 只管样式，不干预窗口位置/大小/最大化——用户可以自由拖动调整，不会被拉回布局位置。
pub fn guard_tick(shared: &SharedRef) {
    let (base, placements, guard_on) = {
        let s = shared.lock().unwrap();
        (s.base.clone(), s.placements.clone(), s.guard_on)
    };
    if !guard_on || placements.is_empty() { return; }
    for p in &placements {
        unsafe {
            let h = p.hwnd as HWND;
            if IsWindow(h) == 0 { continue; }
            let style = GetWindowLongPtrW(h, GWL_STYLE);
            if (style & WS_POPUP) != 0 || (style & WS_THICKFRAME) == 0 {
                {
                    let mut s = shared.lock().unwrap();
                    win::fix_style(p.hwnd, &base, &mut s.store);
                    s.persist();
                }
                log_line(&base, &format!("guard: 修复样式 hwnd={}", p.hwnd));
            }
        }
    }
}

pub fn selftest(shared: &SharedRef) -> String {
    let wa = win::primary_work_area();
    let mons = win::all_monitors();
    let wins = win::enum_windows();
    let s = shared.lock().unwrap();
    let mut out = String::new();
    out.push_str(&format!("WinLayout 自检\n  exe: {:?}\n", exe_dir().join("winlayout.exe")));
    out.push_str(&format!("  主工作区: {},{} {}×{}\n  显示器工作区: {:?}\n", wa.x, wa.y, wa.w, wa.h, mons));
    out.push_str(&format!("  可见窗口数: {}\n  布局数: {}\n  开机自启: {}\n", wins.len(), s.store.layouts.len(), autostart_get()));
    out.push_str(&format!("  设置: {:?}\n", s.store.settings));
    out.push_str("  窗口样本（最多 10 个，只读）:\n");
    for w in wins.iter().take(10) {
        out.push_str(&format!("    hwnd={} {} | {} | {} | [{},{} {}×{}]\n",
            w.hwnd, w.title.chars().take(28).collect::<String>(), w.class, w.process,
            w.rect[0], w.rect[1], w.rect[2], w.rect[3]));
    }
    out
}

/// 安全演示：只对自己启动的记事本窗口做 启动->等待->修样式->摆放->还原->关闭 全流程。
pub fn demo(shared: &SharedRef) -> String {
    let layer = Layer {
        name: "记事本（自检）".into(), cmd: "notepad.exe".into(),
        process: "notepad.exe".into(), ..Default::default()
    };
    let mut out = String::new();
    // 只操作本次新出现的窗口，绝不碰用户已有的记事本
    let before: Vec<i64> = win::enum_windows().into_iter().map(|w| w.hwnd).collect();
    let launched = win::launch(&layer);
    out.push_str(&format!("启动 notepad.exe: {}\n", launched));
    let w = win::wait_for_window(&layer, &before, 9000);
    match w {
        Some(w) => {
            out.push_str(&format!("找到窗口 hwnd={} class={} rect={:?}\n", w.hwnd, w.class, w.rect));
            {
                let mut s = shared.lock().unwrap();
                let base = s.base.clone();
                win::fix_style(w.hwnd, &base, &mut s.store);
                s.persist();
            }
            let wa = win::work_area_for(w.hwnd);
            let r = win::side_rect("left", &wa);
            win::place(w.hwnd, r, false);
            let vis = win::visible_rect(w.hwnd);
            out.push_str(&format!("摆放目标 {:?} 实际(可视) [{},{} {}×{}]\n", r, vis[0], vis[1], vis[2], vis[3]));
            let _ = restore_original(shared, Some(w.hwnd));
            out.push_str("已还原原始样式\n");
            unsafe { PostMessageW(w.hwnd as HWND, WM_CLOSE, 0, 0); }
            out.push_str("已关闭自检窗口\n");
        }
        None => out.push_str("超时：没有找到记事本窗口\n"),
    }
    out
}
