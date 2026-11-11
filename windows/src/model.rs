//! 数据模型与持久化：布局方案、应用分层、设置、原始窗口状态。
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ffi::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub snap_enabled: bool,
    pub snap_px: i32,
    pub guard_enabled: bool,
    pub guard_ms: u32,
    pub wait_ms: u32,
    pub fix_style: bool,
    pub launch_if_missing: bool,
    pub auto_start: bool,
    /// 一键关闭所有快捷键
    #[serde(default = "default_true")]
    pub hotkeys_enabled: bool,
    /// 动作 id -> 组合键字符串；显式存空串表示该动作「留空/禁用」，未存则用默认
    #[serde(default)]
    pub hotkeys: HashMap<String, String>,
}
fn default_true() -> bool { true }
impl Default for Settings {
    fn default() -> Self {
        Self {
            snap_enabled: true, snap_px: 12, guard_enabled: true, guard_ms: 300,
            wait_ms: 9000, fix_style: true, launch_if_missing: true, auto_start: false,
            hotkeys_enabled: true, hotkeys: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub cmd: String,
    pub args: String,
    pub workdir: String,
    pub process: String,
    pub class: String,
    pub title: String,
    pub only_if_missing: bool,
    pub fix_style: bool,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            id: new_id(), name: String::new(), cmd: String::new(), args: String::new(),
            workdir: String::new(), process: String::new(), class: String::new(),
            title: String::new(), only_if_missing: true, fix_style: true,
        }
    }
}

/// 一个分区：rect 为相对工作区的归一化比例 [x, y, w, h]。
/// layers 按顺序从底到顶，同一分区内多个应用重叠摆放（多层布局）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tile {
    pub id: String,
    pub rect: [f64; 4],
    pub cascade: [i32; 2],
    pub layers: Vec<Layer>,
}
impl Default for Tile {
    fn default() -> Self {
        Self { id: new_id(), rect: [0.0, 0.0, 0.5, 1.0], cascade: [24, 24], layers: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub name: String,
    pub tiles: Vec<Tile>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            name: "默认".into(),
            tiles: vec![
                Tile { rect: [0.0, 0.0, 0.5, 1.0], ..Default::default() },
                Tile { rect: [0.5, 0.0, 0.5, 1.0], ..Default::default() },
            ],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Original {
    pub style: i64,
    pub ex: i64,
    pub rect: [i32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub settings: Settings,
    pub layouts: Vec<Layout>,
    /// 窗口原始状态（用于跨进程「还原原状」），key 为 hwnd 字符串。
    pub originals: HashMap<String, Original>,
}
impl Default for Store {
    fn default() -> Self { Self { settings: Settings::default(), layouts: vec![Layout::default()], originals: HashMap::new() } }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RectPx { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiState {
    pub settings: Settings,
    pub layouts: Vec<Layout>,
    pub work_area: RectPx,
    pub monitors: Vec<RectPx>,
    pub autostart: bool,
    /// 快捷键动作表（含默认值、当前值、实际生效值）
    pub hotkey_actions: Vec<HotkeyActionDto>,
    /// 诊断开关：设置环境变量 WINLAYOUT_AUTO_DETECT=1 时，前端启动后自动跑一次占用检测
    pub auto_detect: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HotkeyActionDto {
    pub id: String,
    pub label: String,
    pub default_key: String,
    pub key: String,
    pub effective: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WinInfo {
    pub hwnd: i64,
    pub title: String,
    pub class: String,
    pub process: String,
    pub exe: String,
    /// [x, y, w, h]，屏幕像素
    pub rect: [i32; 4],
    pub z: i32,
    pub visible: bool,
}

/// 守护用：某窗口在布局中的目标矩形。
#[derive(Debug, Clone, Default)]
pub struct Placement {
    pub hwnd: i64,
    pub rect: [i32; 4],
}

pub fn new_id() -> String {
    format!("{}{}", chrono::Local::now().format("%H%M%S%f"), (std::process::id() as u64) % 997)
}

pub fn current_exe_path() -> String {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(0, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    from_wide(&buf[..n])
}

pub fn exe_dir() -> PathBuf {
    let s = current_exe_path();
    Path::new(&s).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

pub fn layouts_path(base: &Path) -> PathBuf { base.join("layouts.json") }
pub fn log_path(base: &Path) -> PathBuf { base.join("winlayout.log") }
pub fn cli_output_path(base: &Path) -> PathBuf { base.join("cli-output.txt") }

pub fn load_store(base: &Path) -> Store {
    let p = layouts_path(base);
    if let Ok(s) = fs::read_to_string(&p) {
        match serde_json::from_str::<Store>(&s) {
            Ok(st) => return st,
            Err(e) => log_line(base, &format!("layouts.json 解析失败: {}（改用默认布局）", e)),
        }
    }
    let st = Store::default();
    save_store(base, &st);
    st
}

pub fn save_store(base: &Path, st: &Store) {
    let p = layouts_path(base);
    let tmp = base.join("layouts.json.tmp");
    match serde_json::to_string_pretty(st) {
        Ok(s) => {
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, &p);
            }
        }
        Err(e) => log_line(base, &format!("布局序列化失败: {}", e)),
    }
}

pub fn log_line(base: &Path, msg: &str) {
    let p = log_path(base);
    if let Ok(md) = fs::metadata(&p) {
        if md.len() > 200_000 { let _ = fs::remove_file(&p); }
    }
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    }
}

pub fn cli_output(base: &Path, text: &str) {
    let _ = fs::write(cli_output_path(base), text);
    // GUI 子系统下 stdout 可能无效，忽略写失败即可（结果同时落在 cli-output.txt）
    use std::io::Write;
    let _ = writeln!(std::io::stdout(), "{}", text);
    log_line(base, &format!("[cli] {}", text.replace('\n', " | ")));
}

// ---------------- 开机自启 ----------------
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "WinLayout";

pub fn autostart_get() -> bool {
    unsafe {
        let mut hk: HKEY = 0;
        let key = wide(RUN_KEY);
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut hk) != ERROR_SUCCESS { return false; }
        let name = wide(RUN_VALUE);
        let mut size: u32 = 0;
        let mut ty: u32 = 0;
        let q = RegQueryValueExW(hk, name.as_ptr(), std::ptr::null_mut(), &mut ty, std::ptr::null_mut(), &mut size);
        RegCloseKey(hk);
        q == ERROR_SUCCESS && size > 2
    }
}

pub fn autostart_set(enabled: bool) {
    unsafe {
        let mut hk: HKEY = 0;
        let key = wide(RUN_KEY);
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_WRITE, &mut hk) != ERROR_SUCCESS { return; }
        let name = wide(RUN_VALUE);
        if enabled {
            let cmd = wide(&format!("\"{}\"", current_exe_path()));
            let bytes: Vec<u8> = cmd.iter().flat_map(|c| c.to_le_bytes()).collect();
            let _ = RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, bytes.as_ptr(), bytes.len() as u32);
        } else {
            let _ = RegDeleteValueW(hk, name.as_ptr());
        }
        RegCloseKey(hk);
    }
}
