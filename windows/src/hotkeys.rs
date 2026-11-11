//! 快捷键定义、组合键解析、占用检测与占用者定位。
//!
//! Windows 没有公开 API 反查 RegisterHotKey 的持有者，因此这里分三层给出结论：
//!  1) 逐键尝试注册 —— 精确判定「可用 / 被占用」；
//!  2) 向所有顶层窗口发 WM_GETHOTKEY —— 能确证用 WM_SETHOTKEY 机制持有同组合的窗口/进程；
//!  3) 枚举进程给出可疑候选 + 前后两次检测的差异推断（关闭可疑程序后重新检测即可定位）。
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ffi::*;
use crate::model::{HotkeyActionDto, Settings};
use crate::win::process_path;

pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default_key: &'static str,
}

pub const ACTIONS: &[Action] = &[
    Action { id: "layout_1", label: "应用布局 1", default_key: "Win+Alt+Shift+1" },
    Action { id: "layout_2", label: "应用布局 2", default_key: "Win+Alt+Shift+2" },
    Action { id: "layout_3", label: "应用布局 3", default_key: "Win+Alt+Shift+3" },
    Action { id: "layout_4", label: "应用布局 4", default_key: "Win+Alt+Shift+4" },
    Action { id: "layout_5", label: "应用布局 5", default_key: "Win+Alt+Shift+5" },
    Action { id: "layout_6", label: "应用布局 6", default_key: "Win+Alt+Shift+6" },
    Action { id: "layout_7", label: "应用布局 7", default_key: "Win+Alt+Shift+7" },
    Action { id: "layout_8", label: "应用布局 8", default_key: "Win+Alt+Shift+8" },
    Action { id: "layout_9", label: "应用布局 9", default_key: "Win+Alt+Shift+9" },
    Action { id: "arrange_left", label: "左半屏（方向键）", default_key: "Win+Alt+Left" },
    Action { id: "arrange_right", label: "右半屏（方向键）", default_key: "Win+Alt+Right" },
    Action { id: "arrange_full", label: "铺满工作区（方向键）", default_key: "Win+Alt+Up" },
    Action { id: "arrange_restore", label: "还原原状（方向键）", default_key: "Win+Alt+Down" },
    Action { id: "legacy_left", label: "左半屏（旧版键位）", default_key: "Win+Alt+1" },
    Action { id: "legacy_right", label: "右半屏（旧版键位）", default_key: "Win+Alt+2" },
    Action { id: "legacy_full", label: "铺满工作区（旧版键位）", default_key: "Win+Alt+3" },
    Action { id: "legacy_restore", label: "还原原状（旧版键位）", default_key: "Win+Alt+4" },
    Action { id: "lock_foreground", label: "锁定前台窗口为目标", default_key: "Win+Alt+0" },
    Action { id: "toggle_guard", label: "保持状态开关", default_key: "Win+Alt+G" },
    Action { id: "window_info", label: "窗口信息", default_key: "Win+Alt+I" },
    Action { id: "save_current", label: "保存当前布局", default_key: "Win+Alt+S" },
    Action { id: "restore_original", label: "还原原状", default_key: "Win+Alt+R" },
];

pub fn action_by_id(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// 未配置 -> 默认；显式存空串 -> 留空禁用。
pub fn effective_key(settings: &Settings, id: &str) -> String {
    match settings.hotkeys.get(id) {
        Some(v) => v.trim().to_string(),
        None => action_by_id(id).map(|a| a.default_key.to_string()).unwrap_or_default(),
    }
}
pub fn actions_state(settings: &Settings, effective: &HashMap<String, String>) -> Vec<HotkeyActionDto> {
    ACTIONS.iter().map(|a| HotkeyActionDto {
        id: a.id.to_string(),
        label: a.label.to_string(),
        default_key: a.default_key.to_string(),
        key: settings.hotkeys.get(a.id).cloned().unwrap_or_else(|| a.default_key.to_string()),
        effective: effective.get(a.id).cloned().unwrap_or_default(),
    }).collect()
}

/// 组合键字符串 -> (修饰符, 虚拟键)。例如 "Win+Alt+Shift+K" / "Ctrl+Alt+F12"。
pub fn parse(text: &str) -> Result<(u32, u32), String> {
    let parts: Vec<String> = text.split('+')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() { return Err("组合键为空".into()); }
    let mut mods = 0u32;
    let mut key: Option<String> = None;
    for p in &parts {
        match p.as_str() {
            "WIN" | "SUPER" | "META" | "WINDOWS" => mods |= MOD_WIN,
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "ALT" => mods |= MOD_ALT,
            "SHIFT" => mods |= MOD_SHIFT,
            _ => {
                if key.is_some() { return Err("只能有一个主键".into()); }
                key = Some(p.clone());
            }
        }
    }
    let k = key.ok_or_else(|| "缺少主键".to_string())?;
    let vk = vk_from_token(&k).ok_or_else(|| format!("无法识别主键「{}」", k))?;
    if mods == 0 { return Err("至少需要一个修饰键（Win / Ctrl / Alt / Shift）".into()); }
    Ok((mods, vk))
}

fn vk_from_token(t: &str) -> Option<u32> {
    if t.len() == 1 {
        let c = t.chars().next().unwrap();
        if c.is_ascii_digit() { return Some(0x30 + c as u32 - '0' as u32); }
        if c.is_ascii_alphabetic() { return Some(0x41 + c.to_ascii_uppercase() as u32 - 'A' as u32); }
        return match c {
            ' ' => Some(0x20), '-' => Some(0xBD), '=' => Some(0xBB),
            '[' => Some(0xDB), ']' => Some(0xDD), ';' => Some(0xBA),
            '\'' => Some(0xDE), ',' => Some(0xBC), '.' => Some(0xBE),
            '/' => Some(0xBF), '\\' => Some(0xDC), '`' => Some(0xC0),
            _ => None,
        };
    }
    if let Some(rest) = t.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) { return Some(0x6F + n); }
        }
    }
    Some(match t {
        "LEFT" | "ARROWLEFT" => 0x25, "UP" | "ARROWUP" => 0x26,
        "RIGHT" | "ARROWRIGHT" => 0x27, "DOWN" | "ARROWDOWN" => 0x28,
        "SPACE" | "SPACEBAR" => 0x20, "ENTER" | "RETURN" => 0x0D,
        "TAB" => 0x09, "ESC" | "ESCAPE" => 0x1B, "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2E, "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24, "END" => 0x23, "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22, "PRINTSCREEN" | "PRTSC" => 0x2C,
        "SCROLLLOCK" => 0x91, "PAUSE" => 0x13, "CAPSLOCK" => 0x14,
        "NUMLOCK" => 0x90, "NUMPAD0" => 0x60,
        _ => return None,
    })
}

// ---------------- 检测结果 ----------------
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Candidate {
    pub pid: u32,
    pub process: String,
    pub exe: String,
    pub title: String,
    pub class: String,
    pub visible: bool,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectItem {
    pub id: String,
    pub label: String,
    pub key: String,
    /// ok / occupied / empty / invalid / off
    pub status: String,
    pub error: u32,
    pub owners: Vec<Candidate>,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectReport {
    pub seq: u64,
    pub items: Vec<DetectItem>,
    pub candidates: Vec<Candidate>,
    pub likely_owners: Vec<Candidate>,
    pub note: String,
    pub ok_count: usize,
    pub occupied_count: usize,
    pub empty_count: usize,
    pub invalid_count: usize,
    pub off_count: usize,
}

struct RawWindow {
    pid: u32,
    class: String,
    title: String,
    visible: bool,
}

struct SetHotkey {
    mods: u32,
    vk: u32,
    pid: u32,
    class: String,
    title: String,
}

struct ScanOut {
    windows: Vec<RawWindow>,
    set_hotkeys: Vec<SetHotkey>,
}

unsafe extern "system" fn scan_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    let out = &mut *(lparam as *mut ScanOut);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() { return 1; }
    let mut cb = [0u16; 256];
    GetClassNameW(hwnd, cb.as_mut_ptr(), 256);
    let n = GetWindowTextLengthW(hwnd);
    let mut tb = vec![0u16; (n + 2) as usize];
    GetWindowTextW(hwnd, tb.as_mut_ptr(), (n + 2) as i32);
    let class = from_wide(&cb);
    let title = from_wide(&tb);
    out.windows.push(RawWindow {
        pid, class: class.clone(), title: title.clone(),
        visible: IsWindowVisible(hwnd) != 0,
    });
    // WM_GETHOTKEY：查询该窗口设置的「窗口激活热键」（能确证占用者的一种机制）
    let mut res: usize = 0;
    SendMessageTimeoutW(hwnd, WM_GETHOTKEY, 0, 0,
        SMTO_ABORTIFHUNG | SMTO_BLOCK, 20, &mut res);
    if res != 0 {
        let vk = (res & 0xFF) as u32;
        let hf = ((res >> 8) & 0xFF) as u32;
        let mut mods = 0u32;
        if (hf & HOTKEYF_ALT) != 0 { mods |= MOD_ALT; }
        if (hf & HOTKEYF_CONTROL) != 0 { mods |= MOD_CONTROL; }
        if (hf & HOTKEYF_SHIFT) != 0 { mods |= MOD_SHIFT; }
        out.set_hotkeys.push(SetHotkey { mods, vk, pid, class, title });
    }
    1
}

const KNOWN_HOTKEY_APPS: &[&str] = &[
    "autohotkey", "ahk", "powertoys", "fancyzones", "snipaste", "pixpin",
    "sharex", "greenshot", "wox", "utools", "quicker", "listary", "everything",
    "obs64", "obs32", "bandicam", "code.exe", "zcode", "workbuddy", "cursor",
    "idea64", "pycharm", "devenv", "wechat", "weixin", "qq.exe", "dingtalk",
    "sogou", "baiduinput", "cloudmusic", "qqmusic", "spotify", "discord",
    "telegram", "steam", "amdrsserv", "rtss", "msiafterburner", "nvidia",
];
const SYSTEM_PROCS: &[&str] = &[
    "explorer.exe", "applicationframehost.exe", "shellexperiencehost.exe",
    "startmenuexperiencehost.exe", "textinputhost.exe", "searchhost.exe",
    "dwm.exe", "csrss.exe", "svchost.exe", "winlogon.exe", "services.exe",
];

fn candidate_from(w: &RawWindow) -> Candidate {
    let (process, exe) = process_path(w.pid);
    let lower = format!("{} {}", process.to_lowercase(), exe.to_lowercase());
    let p = process.to_lowercase();
    let mut score = 1;
    let mut reasons: Vec<String> = Vec::new();
    const TOOLS: &[&str] = &["powertoys", "pixpin", "snipaste", "sharex", "greenshot",
        "autohotkey", "ahk", "utools", "quicker", "wox", "listary", "fancyzones", "everything"];
    const IDES: &[&str] = &["zcode", "workbuddy", "code.exe", "cursor", "idea64",
        "pycharm", "devenv", "visualstudio"];
    const IME: &[&str] = &["sogou", "baiduinput", "msime", "textinputhost"];
    if TOOLS.iter().any(|k| lower.contains(k)) {
        score += 80;
        reasons.push("全局热键 / 窗口管理工具（重点怀疑）".into());
    } else if IDES.iter().any(|k| lower.contains(k)) {
        score += 50;
        reasons.push("IDE / 编辑器（常注册 Win+Alt 组合）".into());
    } else if IME.iter().any(|k| lower.contains(k)) {
        score += 30;
        reasons.push("输入法组件".into());
    } else if KNOWN_HOTKEY_APPS.iter().any(|k| lower.contains(k)) {
        score += 25;
        reasons.push("常见热键程序".into());
    }
    if ["powertoys.exe", "pixpin.exe", "snipaste.exe", "autohotkey.exe"].iter().any(|k| p == *k) {
        score += 20;
        reasons.push("工具主进程".into());
    }
    if SYSTEM_PROCS.iter().any(|k| process.eq_ignore_ascii_case(k)) {
        score += 5;
        reasons.push("系统组件（不建议结束）".into());
    }
    if w.class.to_lowercase().contains("autohotkey") {
        score += 30;
        reasons.push("AutoHotkey 脚本窗口".into());
    }
    if !w.visible {
        score += 12;
        reasons.push("后台 / 隐藏窗口（热键常驻程序特征）".into());
    } else {
        score += 3;
        reasons.push("有可见窗口".into());
    }
    Candidate {
        pid: w.pid, process, exe, title: w.title.clone(), class: w.class.clone(),
        visible: w.visible, score,
        reason: if reasons.is_empty() { "普通进程".into() } else { reasons.join(" · ") },
    }
}

fn scan_candidates() -> (Vec<Candidate>, Vec<SetHotkey>) {
    let mut out = ScanOut { windows: Vec::new(), set_hotkeys: Vec::new() };
    unsafe { EnumWindows(scan_cb, &mut out as *mut _ as LPARAM); }
    let mut map: HashMap<u32, Candidate> = HashMap::new();
    for w in &out.windows {
        if w.pid == 0 { continue; }
        let c = candidate_from(w);
        map.entry(w.pid).and_modify(|old| {
            if c.score > old.score { *old = c.clone(); }
        }).or_insert(c);
    }
    let mut list: Vec<Candidate> = map.into_values().collect();
    list.sort_by(|a, b| b.score.cmp(&a.score).then(a.process.cmp(&b.process)));
    (list, out.set_hotkeys)
}

fn owner_from_set_hotkey(s: &SetHotkey) -> Candidate {
    let (process, exe) = process_path(s.pid);
    Candidate {
        pid: s.pid, process, exe, title: s.title.clone(), class: s.class.clone(),
        visible: true, score: 100,
        reason: "该窗口通过 WM_SETHOTKEY 设置了同一组合（确定的占用者）".into(),
    }
}

/// 请求某个进程的全部顶层窗口关闭（用于「关闭候选 → 重新检测」的排除法）。
unsafe extern "system" fn close_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    let target = lparam as u32;
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == target { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
    1
}
pub fn close_windows_of(pid: u32) {
    unsafe { EnumWindows(close_cb, pid as LPARAM); }
}

/// 逐键探测（调用方需保证本程序当前没有注册这些热键，否则会「被自己占用」）。
pub fn detect(settings: &Settings, seq: u64) -> DetectReport {
    let (candidates, set_hotkeys) = scan_candidates();
    let mut report = DetectReport { seq, candidates, ..Default::default() };
    for (i, a) in ACTIONS.iter().enumerate() {
        let key = effective_key(settings, a.id);
        let mut item = DetectItem {
            id: a.id.to_string(), label: a.label.to_string(), key: key.clone(),
            status: String::new(), error: 0, owners: Vec::new(), note: String::new(),
        };
        if !settings.hotkeys_enabled {
            item.status = "off".into();
            item.note = "总开关已关闭".into();
        } else if key.is_empty() {
            item.status = "empty".into();
            item.note = "留空（已禁用）".into();
        } else {
            match parse(&key) {
                Err(e) => { item.status = "invalid".into(); item.note = e; }
                Ok((mods, vk)) => {
                    let probe_id = PROBE_ID_BASE + i as i32;
                    unsafe {
                        let mut ok = RegisterHotKey(0, probe_id, mods | MOD_NOREPEAT, vk) != 0;
                        let first_err = GetLastError();
                        if !ok { ok = RegisterHotKey(0, probe_id, mods, vk) != 0; }
                        if ok {
                            UnregisterHotKey(0, probe_id);
                            item.status = "ok".into();
                        } else {
                            item.status = "occupied".into();
                            item.error = if GetLastError() != 0 { GetLastError() } else { first_err };
                        }
                    }
                    // WM_GETHOTKEY 确证（该机制不支持 Win 键，故含 Win 的组合不会命中）
                    let want = mods & (MOD_ALT | MOD_CONTROL | MOD_SHIFT);
                    for s in &set_hotkeys {
                        if s.vk == vk && s.mods == want {
                            item.owners.push(owner_from_set_hotkey(s));
                        }
                    }
                    if !item.owners.is_empty() {
                        item.note = "已定位到设置该组合的窗口".into();
                    } else if item.status == "occupied" {
                        item.note = format!("被其它程序注册（Win32 错误 {}）", item.error);
                    }
                }
            }
        }
        report.items.push(item);
    }
    for i in &report.items {
        match i.status.as_str() {
            "ok" => report.ok_count += 1,
            "occupied" => report.occupied_count += 1,
            "empty" => report.empty_count += 1,
            "invalid" => report.invalid_count += 1,
            "off" => report.off_count += 1,
            _ => {}
        }
    }
    report
}

/// 排除法辅助：请求关闭窗口或结束进程。系统组件与自身一律拒绝。
pub fn stop_process(pid: u32, force: bool) -> Result<String, String> {
    if pid == unsafe { GetCurrentProcessId() } { return Err("不能结束本程序".into()); }
    let (process, exe) = process_path(pid);
    if process.is_empty() { return Err(format!("找不到 PID {}（可能已退出）", pid)); }
    const BLOCKED: &[&str] = &[
        "explorer.exe", "dwm.exe", "csrss.exe", "winlogon.exe", "services.exe",
        "svchost.exe", "applicationframehost.exe", "shellexperiencehost.exe",
        "startmenuexperiencehost.exe", "textinputhost.exe", "searchhost.exe",
    ];
    if BLOCKED.iter().any(|b| process.eq_ignore_ascii_case(b)) {
        return Err(format!("{} 是系统组件，已拒绝自动结束", process));
    }
    if force {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h == 0 { return Err(format!("无法打开进程 {}（权限不足或已退出）", pid)); }
            let ok = TerminateProcess(h, 1);
            CloseHandle(h);
            if ok == 0 { return Err(format!("结束进程失败，Win32 错误 {}", GetLastError())); }
        }
    } else {
        close_windows_of(pid);
    }
    let act = if force { "强制结束" } else { "请求关闭窗口" };
    Ok(format!("已{} {} (PID {}) {}", act, process, pid, exe))
}

/// 对比两次检测：若某键从「被占用」变「可用」，且某些候选进程恰好消失，即可推断占用者。
pub fn infer_owners(prev: &DetectReport, cur: &mut DetectReport) {
    let became_ok: usize = prev.items.iter()
        .filter(|p| p.status == "occupied")
        .filter(|p| cur.items.iter().any(|c| c.id == p.id && c.status == "ok"))
        .count();
    let now: HashSet<u32> = cur.candidates.iter().map(|c| c.pid).collect();
    let gone: Vec<Candidate> = prev.candidates.iter()
        .filter(|c| !now.contains(&c.pid))
        .cloned().collect();
    if became_ok == 0 {
        if !gone.is_empty() {
            cur.note = format!("对比上次：{} 个进程已退出，但没有快捷键恢复可用 —— 它们不是占用者，继续排查下一个候选。", gone.len());
        }
        return;
    }
    if gone.is_empty() {
        cur.note = format!("对比上次：{} 个快捷键恢复可用，但候选进程无明显变化；可关闭可疑程序后再检测。", became_ok);
    } else {
        cur.likely_owners = gone;
        cur.note = format!("对比上次：{} 个快捷键恢复可用，同时以下进程已退出 —— 它们很可能就是占用者。", became_ok);
    }
}

pub fn format_report(r: &DetectReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("快捷键检测：可用 {} / 被占用 {} / 留空 {} / 无效 {} / 已关闭 {}\n",
        r.ok_count, r.occupied_count, r.empty_count, r.invalid_count, r.off_count));
    for i in &r.items {
        let tag = match i.status.as_str() {
            "ok" => "可用", "occupied" => "被占用", "empty" => "留空",
            "invalid" => "格式无效", "off" => "总开关关闭", _ => "?",
        };
        out.push_str(&format!("  [{:<8}] {:<22} {:<24} {}", tag, i.label,
            if i.key.is_empty() { "(空)" } else { i.key.as_str() }, i.note));
        out.push('\n');
        for o in &i.owners {
            out.push_str(&format!("      └ 确证占用者: {} (PID {}) {}\n", o.process, o.pid, o.exe));
        }
    }
    if !r.likely_owners.is_empty() {
        out.push_str(&format!("\n推断占用者：{}\n{}\n", r.note,
            r.likely_owners.iter().map(|c| format!("  - {} (PID {}) {}", c.process, c.pid, c.exe))
                .collect::<Vec<_>>().join("\n")));
    } else if !r.note.is_empty() {
        out.push_str(&format!("\n{}\n", r.note));
    }
    out.push_str("\n可疑进程候选（按可疑度，前 14 个）：\n");
    for c in r.candidates.iter().take(14) {
        out.push_str(&format!("  {:>3}  {:<34} PID {:<8} {}\n", c.score, c.process, c.pid, c.reason));
        if !c.exe.is_empty() { out.push_str(&format!("       {}\n", c.exe)); }
    }
    out.push_str("\n提示：Windows 无公开 API 反查 RegisterHotKey 的持有者。关闭可疑程序后点「重新检测」，\n若某键从被占用变可用，工具会自动把消失的进程标为占用者。\n");
    out
}
