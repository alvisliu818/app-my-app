# WinLayout · 窗口布局管家

Tauri 2 + Rust（Win32 API）实现的窗口布局管理器，放在 `app-my-app/windows/`。
前端是纯静态 HTML/JS/CSS（无需 Node 依赖，`withGlobalTauri` 注入 IPC），
后端 Rust 负责窗口枚举、样式修复、启动应用、摆放窗口、托盘、热键、贴边钩子。

`WindowHalf.ahk` / `WindowHalf.exe` 是此前的 AHK v2 版，保留作参考，不再需要运行。

---

## 一、原先（AHK 版 WindowHalf）的能力，全部保留

| 旧功能 | 本版实现 |
| --- | --- |
| 无边框 / POPUP 窗口改成可缩放 | 去 `WS_POPUP`、加 `WS_OVERLAPPEDWINDOW`、`SWP_FRAMECHANGED` 重算非客户区、去置顶、`ClipCursor(0)` 解鼠标锁 |
| 左半屏 / 右半屏 / 铺满工作区 / 还原 | `Win+Alt+1/2/3/4`、`Win+Alt+方向键`、托盘菜单、命令行 `--left/--right/--full/--restore` |
| 目标窗口锁定 | `Win+Alt+0` 锁定前台窗口；自动挑选优先匹配 `RedEagle.Monitor` / `REDAgent.exe`，其次「无边框且覆盖大部分工作区」，最后前台窗口 |
| 保持状态守护 | 每 300ms 幂等检查：样式被改回 POPUP 或窗口被放大回全屏时自动恢复（`guard_tick`），`Win+Alt+G` 开关 |
| 原始样式/矩形可跨进程还原 | 记录在 `layouts.json` 的 `originals`，`--restore` / `Win+Alt+↓` 还原 |
| 托盘、开机自启、日志、窗口信息 | 托盘右键菜单、HKCU Run 自启项、`winlayout.log`、`Win+Alt+I` 信息 |
| DPI 感知、显示器工作区 | `SetProcessDpiAwarenessContext(-4)` + `MonitorFromWindow/GetMonitorInfoW` |
| 命令行单次模式 | 见下方命令行一节 |

## 二、新功能

1. **布局保存**：多个命名布局保存在程序目录 `layouts.json`（归一化比例坐标，换分辨率也能还原）；编辑器自动保存 + 显式「保存」按钮（`Ctrl+S`）。
2. **★ 保存当前布局**：一键把桌面上现有窗口的**位置、大小、Z 序、启动命令（exe 全路径）、进程名/类名**抓取成布局；可勾选窗口，**重叠窗口自动合并为同一分区的多个层**。工具栏按钮、托盘菜单、`Win+Alt+S`、命令行 `--capture` 都能用。
3. **应用布局能打开对应 app 并布局好**：按 进程名 → 类名 → 标题 匹配已开窗口；没找到就用 `ShellExecuteW` 启动命令、轮询等窗口出现（超时可设），再修样式、按矩形 + Z 序 + 层叠偏移摆好。
4. **支持 app 重叠多层布局**：一个分区可放多个应用层（layers 从底到顶），应用时按序抬 Z 序并做层叠偏移；画布上以层叠卡片展示，可上移/下移/置顶/删除/编辑。
5. **拖动改变布局**：画布里拖动分区移动、拖边缘/四角改大小；开启「联动相邻分区」时，拖动共享边界会带着邻居一起变；可拆分、新建、删除、置顶分区，或双击空白新增。
6. **支持贴边**：
   - 编辑器：拖动时吸附屏幕边、中线、其它分区边缘（阈值可设），显示辅助线；
   - 桌面实时拖动：低级鼠标钩子在松手时把贴近屏幕边的窗口吸附为左/右半屏、顶部铺满、下半屏或四分之一屏；**按住 Ctrl 拖动**则落入当前布局里最近的分区；
   - 默认热键 `Win+Alt+方向键` 直接贴边（可自定义）。
7. **快捷键自定义 / 留空 / 全关 / 占用检测**：22 个动作逐项改键，留空即禁用，总开关一键全关；
   「检测占用」给出逐键状态、WM_GETHOTKEY 确证结果、可疑进程候选，并支持「关闭候选 → 自动重测 → 推断占用者」的排除法（详见第三节）。

## 三、快捷键（可自定义 / 可留空 / 可全关 / 可检测占用）

点顶部「快捷键」按钮（或托盘「快捷键设置 / 检测占用…」）打开设置面板：

- **逐项自定义**：输入框直接写组合键，如 `Win+Alt+K`、`Ctrl+Alt+F12`、`Win+Shift+Left`；
  支持 Win/Ctrl/Alt/Shift + 数字、字母、F1~F24、方向键、Space/Enter/Tab/Esc/Home/End/PageUp/PageDown/PrintScreen 等。
- **留空 = 禁用该动作**；「全部留空」一键清空，「恢复默认」一键还原。
- **总开关**：关掉「启用全部快捷键」后一个都不注册（托盘菜单、界面按钮、命令行不受影响）。
- 保存后**立即生效**：系统线程注销全部旧热键并严格按设定值重新注册；每行显示「已生效 / 未生效」。
- **不自动回退**：任何键位（包括默认值）注册失败就如实记为被占用，不会偷偷换成别的组合。

默认键位（只是初始值，全部可改）：

| 快捷键 | 作用 |
| --- | --- |
| `Win+Alt+Shift+1~9` | 应用第 N 个布局 |
| `Win+Alt+S` | 保存当前布局（时间戳命名） |
| `Win+Alt+1 / 2 / 3 / 4` | 旧版：左半屏 / 右半屏 / 铺满 / 还原 |
| `Win+Alt+← / → / ↑ / ↓` | 左半屏 / 右半屏 / 铺满 / 还原 |
| `Win+Alt+0` | 锁定前台窗口为目标 |
| `Win+Alt+G` | 保持状态守护开关 |
| `Win+Alt+I` | 窗口信息（写入日志/状态栏） |
| `Win+Alt+R` | 还原原状 |
| 拖动窗口时按住 `Ctrl` | 松手后落入当前布局最近的分区 |

关闭主窗口 = 最小化到托盘，进程常驻；托盘「退出」才真正结束。

> **被占用时不自动回退**：所有键位都严格按设定值注册；`RegisterHotKey` 返回
> `1409 ERROR_HOTKEY_ALREADY_REGISTERED`（本机实测 `Win+Alt` 组被 PowerToys / PixPin / IDE 等占用）
> 就记为失败，该行显示「未生效」。请用「检测占用」定位占用者，然后把这个键改成空闲组合或留空；
> 托盘菜单、界面按钮与命令行始终可用，不受影响。

### 快捷键占用检测

点「检测占用」（或命令行 `--hotkeys`）会逐键试注册，给出：可用 / 被占用 / 留空 / 格式无效 / 已关闭。
检测时先临时注销本程序的热键，因此不会把「自己」算成占用者。

Windows **没有公开 API** 反查 `RegisterHotKey` 的持有者，工具用三层信息定位占用程序：

1. **确证**：向所有顶层窗口发 `WM_GETHOTKEY`，找到用 `WM_SETHOTKEY` 机制设置了同一组合的窗口，
   直接给出进程名、PID、exe 路径；
2. **候选**：枚举所有进程（含隐藏/托盘窗口）按可疑度打分，标出常见的全局热键工具、IDE、输入法、
   系统组件，列出 PID 与 exe 路径；
3. **差异推断（一键闭环）**：候选进程每行有「关闭窗口 / 结束进程」按钮（需二次确认，系统组件会被拒绝）。
   点击后工具自动重新检测；若某键从「被占用」变「可用」，就会把这次消失的进程标为**推断的占用者**；
   若占用状态没变，会提示「它不是占用者，继续排查下一个」。逐个排除即可定位到具体程序。

> 完全精确的持有者查询需要向所有 GUI 进程注入 DLL 并监听 `WM_HOTKEY`
> （Hotkey Detective / hkdetect 的做法）。本工具刻意不注入其它进程；
> 「关闭可疑程序 → 重新检测」可在不注入的前提下定位到具体程序。

## 四、命令行

```bat
winlayout.exe --left            :: 左半屏（自动挑目标窗口）
winlayout.exe --right
winlayout.exe --full
winlayout.exe --restore
winlayout.exe --apply "开发"     :: 应用布局（会启动缺失的 app）
winlayout.exe --capture [名称]   :: 保存当前桌面为布局
winlayout.exe --list             :: 列出布局
winlayout.exe --hotkeys          :: 逐键检测占用 + 可疑进程候选，结果写 cli-output.txt
winlayout.exe --stop-probe <PID> [--force]  :: 检测辅助（谨慎）：关闭/结束可疑进程后再 --hotkeys 对比
winlayout.exe --selftest         :: 只读自检（窗口/工作区/显示器/设置），结果写 cli-output.txt
winlayout.exe --demo             :: 用自己启动的记事本跑一遍 启动→摆放→还原→关闭 全流程
winlayout.exe --info | --lock
```

CLI 输出同时写入 `cli-output.txt` 与 `winlayout.log`（GUI 子系统下不一定有控制台）。

## 五、构建

```bat
cd E:\HOME\Code\app\app-my-app\windows
cargo build --release
```

本目录已附带编译好的 `winlayout.exe`（5.2 MB）。程序以自身所在目录作为数据目录
（`layouts.json` / `winlayout.log` / `cli-output.txt`），所以从本目录直接运行即可；
若移动 exe，布局数据不会跟着走。改代码后把 `target\release\winlayout.exe` 复制回本目录。

- 需要 MSVC 工具链 + Windows 10/11 的 WebView2 Runtime（本机已具备）。
- crates.io 官方 index 在本机解析极慢，已把 rsproxy 稀疏镜像配置到**全局** `~/.cargo/config.toml`；
  项目内 `.cargo/config.toml` 另留一份相同配置（项目级优先，便于仓库 clone 后直接构建），删任意一份都不影响另一份生效。
- 依赖：`tauri 2`、`serde`/`serde_json`、`chrono`；Win32 调用全部手写 FFI（`src/ffi.rs`），不引入 windows-sys。

## 六、代码结构

| 文件 | 职责 |
| --- | --- |
| `src/ffi.rs` | Win32 API 声明、常量、结构体、宽字符串工具 |
| `src/model.rs` | 数据模型、`layouts.json` 持久化、日志、开机自启 |
| `src/win.rs` | 窗口枚举/信息、样式修复、摆放、启动、工作区、窗口匹配 |
| `src/engine.rs` | 应用布局、保存当前布局、快速分屏、贴边、守护、自检/demo |
| `src/hotkeys.rs` | 22 个动作定义、组合键解析、逐键占用探测、WM_GETHOTKEY 确证、候选进程评分与差异推断 |
| `src/sys.rs` | 系统线程：托盘、热键、鼠标钩子、定时器 |
| `src/commands.rs` | Tauri 命令 |
| `src/main.rs` | 入口、CLI、Tauri Builder |
| `ui/` | 可视化布局编辑器（画布拖动/贴边/多层、属性面板、对话框） |

## 七、注意

- 应用布局会移动/抬高其它程序窗口，属于预期行为；`fix_style` 只改窗口样式，不改程序内部数据，随时可「还原原状」。
- 「保存当前布局」记录的是窗口所属进程的 exe 路径；UWP/系统组件或无法查询路径的窗口可能只记录到进程名，之后启动时请在属性面板补一条启动命令。
- 多显示器：矩形以主工作区归一化；实时贴边按窗口所在显示器的工作区计算。
- 旧 AHK 版有「以管理员重启」菜单项；实测普通权限即可改样式（旧版日志 admin=0 也成功），
  所以本版不自动 UAC 提权；个别特权进程窗口若改不动，右键 `winlayout.exe` → 以管理员身份运行即可。
- Windows 自带「贴靠窗口」若同时开启，拖到屏幕边缘可能与本工具叠加生效，可在设置里关掉本工具的贴边。
