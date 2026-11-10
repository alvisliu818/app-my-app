#Requires AutoHotkey v2.0
#SingleInstance Force

; WindowHalf - 让无边框全屏窗口恢复可缩放，并快速分屏
;   Win+Alt+1  左半屏        Win+Alt+2  右半屏     Win+Alt+3  铺满工作区
;   Win+Alt+4  还原原状      Win+Alt+0  把前台窗口设为目标
;   Win+Alt+G  保持状态开关  Win+Alt+I  窗口信息
; 命令行（可用于快捷方式，一次操作后退出）：
;   WindowHalf.exe --left | --right | --full | --restore
; 目标窗口自动锁定：优先匹配 RedEagle.Monitor / REDAgent.exe，
; 其次自动找「无边框且覆盖大部分工作区」的窗口，最后才用前台窗口。
; 日志与状态写在脚本同目录：WindowHalf.log / WindowHalf.state
; 以自己设备、允许调整窗口的环境为使用前提。

APP := "WindowHalf"
RunKey := "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
LogFile := A_ScriptDir "\WindowHalf.log"
StateFile := A_ScriptDir "\WindowHalf.state"
gHwnd := 0
gGuard := false
gWarned := false
gTargetRect := []
gOriginal := Map()

try {
    DllCall("user32\SetProcessDpiAwarenessContext", "Ptr", -4)
} catch {
    try DllCall("shcore\SetProcessDpiAwareness", "Int", 2)
}

; 未编译的 .ahk 直接运行时自动提权；编译版不自动提权，避免开机自启每次弹 UAC。
if !A_IsAdmin && !A_IsCompiled && !(A_Args.Length && A_Args[1] = "--no-elevate") {
    Run('*RunAs "' A_AhkPath '" "' A_ScriptFullPath '"')
    ExitApp
}

BuildTray()
Log("started admin=" A_IsAdmin " compiled=" A_IsCompiled " screen=" A_ScreenWidth "x" A_ScreenHeight)

; 命令行单次模式：执行一个操作后退出，方便做快捷方式 / 自检。
if A_Args.Length && A_Args[1] != "--no-elevate" {
    op := A_Args[1]
    Log("cli " op)
    try {
        switch op {
            case "--left":   Arrange("left")
            case "--right":  Arrange("right")
            case "--full":   Arrange("full")
            case "--restore": RestoreOriginal()
            default: Log("cli unknown " op)
        }
    } catch {
        Log("cli error handled, exiting")
    }
    ExitApp
}

TrayTip("已启动 · Win+Alt+1/2 左右半屏，3 铺满，4 还原，I 信息", APP)
return

; ---------------- 热键 ----------------
#!1::Arrange("left")
#!2::Arrange("right")
#!3::Arrange("full")
#!4::RestoreOriginal()
#!0::LockForeground()
#!g::ToggleGuard()
#!i::ShowInfo()

; ---------------- 托盘 ----------------
BuildTray() {
    t := A_TrayMenu
    t.Delete()
    t.Add("左半屏", (*) => Arrange("left"))
    t.Add("右半屏", (*) => Arrange("right"))
    t.Add("铺满工作区", (*) => Arrange("full"))
    t.Add("还原原状", (*) => RestoreOriginal())
    t.Add()
    t.Add("锁定前台窗口为目标", (*) => LockForeground())
    t.Add("保持状态", (*) => ToggleGuard())
    t.Add("开机自启", (*) => ToggleAutoStart())
    t.Add()
    t.Add("窗口信息", (*) => ShowInfo())
    t.Add("打开日志", (*) => OpenLog())
    if !A_IsAdmin
        t.Add("以管理员重启", (*) => RestartAsAdmin())
    t.Add("退出", (*) => ExitApp())
    A_IconTip := "WindowHalf · Win+Alt+1/2 分屏，3 铺满，4 还原，0 锁定目标"
    SyncMenu()
}

SyncMenu() {
    global gGuard
    t := A_TrayMenu
    if gGuard
        t.Check("保持状态")
    else
        t.Uncheck("保持状态")
    if IsAutoStart()
        t.Check("开机自启")
    else
        t.Uncheck("开机自启")
}

OpenLog() {
    global LogFile
    if FileExist(LogFile)
        Run("notepad.exe `"" LogFile "`"")
}

Log(s) {
    global LogFile
    try {
        if FileExist(LogFile) && FileGetSize(LogFile, "K") > 200
            FileDelete(LogFile)
        FileAppend(A_Now " " s "`n", LogFile, "UTF-8")
    } catch {
    }
}

; ---------------- 目标窗口 ----------------
IsSystemWindow(cls, proc) {
    if (proc = "explorer.exe")
        return true
    static bad := ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd",
        "DuiHostWnd", "Windows.UI.Core.CoreWindow", "ApplicationFrameWindow",
        "NotifyIconOverflowWindow", "Tao Thread", "Qt5", "Chrome_WidgetWin"]
    for b in bad {
        if InStr(cls, b)
            return true
    }
    return false
}

PickTarget() {
    global gHwnd
    if gHwnd && WinExist("ahk_id " gHwnd)
        return gHwnd
    best := 0, bestArea := 0
    for hwnd in WinGetList() {
        if !DllCall("user32\IsWindowVisible", "Ptr", hwnd)
            continue
        if DllCall("user32\IsIconic", "Ptr", hwnd)
            continue
        id := "ahk_id " hwnd
        cls := "", proc := ""
        try {
            cls := WinGetClass(id)
            proc := WinGetProcessName(id)
        } catch {
            continue
        }
        if (cls = "RedEagle.Monitor" || proc = "REDAgent.exe") {
            gHwnd := hwnd
            Log("target=exact hwnd=" hwnd " class=" cls " proc=" proc)
            return hwnd
        }
        if IsSystemWindow(cls, proc)
            continue
        st := WinGetStyle(id), ex := WinGetExStyle(id)
        if (ex & 0x80) || (ex & 0x80000) || (ex & 0x20)
            continue
        if (st & 0x40000) || ((st & 0xC00000) = 0xC00000)
            continue
        WinGetPos(&x, &y, &w, &h, id)
        if (w * h > bestArea && w >= A_ScreenWidth * 0.8 && h >= A_ScreenHeight * 0.7)
            best := hwnd, bestArea := w * h
    }
    if best {
        gHwnd := best
        Log("target=auto hwnd=" best)
        return best
    }
    gHwnd := WinExist("A")
    Log("target=fallback-foreground hwnd=" gHwnd)
    return gHwnd
}

LockForeground() {
    global gHwnd, APP
    h := WinExist("A")
    if h {
        gHwnd := h
        Log("target=manual hwnd=" h)
        TrayTip("已把当前前台窗口锁定为目标", APP)
    }
}

; ---------------- 核心 ----------------
Arrange(side) {
    global gHwnd, gGuard, gTargetRect, APP
    hwnd := PickTarget()
    if !hwnd {
        TrayTip("没有找到目标窗口", APP)
        return
    }
    FixStyle(hwnd, true)
    GetWorkArea(hwnd, &L, &T, &R, &B)
    w := R - L, h := B - T
    id := "ahk_id " hwnd
    if (side = "left")
        tx := L, ty := T, tw := w // 2, th := h
    else if (side = "right")
        tx := L + w // 2, ty := T, tw := w // 2, th := h
    else
        tx := L, ty := T, tw := w, th := h
    WinMove(tx, ty, tw, th, id)
    RaiseNoActivate(hwnd)
    gTargetRect := [tx, ty, tw, th]
    Log("arrange " side " rect=" tx "," ty " " tw "x" th)
    if !gGuard {
        gGuard := true
        SetTimer(GuardTick, 300)
        SyncMenu()
    }
}

FixStyle(hwnd, restore := true) {
    global gWarned, gOriginal, APP, StateFile
    WS_POPUP := 0x80000000
    ADD := 0x00CF0000   ; WS_OVERLAPPEDWINDOW = 标题栏 + 系统菜单 + 可缩放边框 + 最小化/最大化
    id := "ahk_id " hwnd
    style := WinGetStyle(id), ex := WinGetExStyle(id)
    WinGetPos(&x, &y, &w, &h, id)
    if !gOriginal.Has(hwnd) {
        o := {style: style, ex: ex, x: x, y: y, w: w, h: h}
        gOriginal[hwnd] := o
        SaveState(hwnd, o)
    }
    if (style & ADD) != ADD || (style & WS_POPUP) {
        newSt := (style & ~WS_POPUP) | ADD
        DllCall("kernel32\SetLastError", "UInt", 0)
        DllCall("user32\SetWindowLongPtrW", "Ptr", hwnd, "Int", -16, "Ptr", newSt, "Ptr")
        err := A_LastError
        FrameChanged(hwnd)
        Log("style 0x" Format("{1:X}", style) " -> 0x" Format("{1:X}", WinGetStyle(id)) " err=" err)
    }
    if (ex & 0x8)
        WinSetAlwaysOnTop(0, id)
    if restore
        WinRestore(id)
    DllCall("user32\ClipCursor", "Ptr", 0)
    if !gWarned && (WinGetStyle(id) & ADD) != ADD {
        gWarned := true
        Log("STYLE FAILED err=" A_LastError)
        TrayTip("改样式失败，请用托盘「以管理员重启」，详见日志", APP)
    }
}

FrameChanged(hwnd) {
    ; SWP_NOSIZE|SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_FRAMECHANGED
    DllCall("user32\SetWindowPos", "Ptr", hwnd, "Ptr", 0,
        "Int", 0, "Int", 0, "Int", 0, "Int", 0, "UInt", 0x37)
}

RaiseNoActivate(hwnd) {
    ; HWND_TOP + SWP_NOSIZE|SWP_NOMOVE|SWP_NOACTIVATE，只提 Z 序，不抢焦点
    DllCall("user32\SetWindowPos", "Ptr", hwnd, "Ptr", 0,
        "Int", 0, "Int", 0, "Int", 0, "Int", 0, "UInt", 0x13)
}

RestoreOriginal() {
    global gHwnd, gOriginal, gGuard, gTargetRect, APP, StateFile
    hwnd := gHwnd && WinExist("ahk_id " gHwnd) ? gHwnd : PickTarget()
    if !hwnd
        return
    o := gOriginal.Has(hwnd) ? gOriginal[hwnd] : LoadState(hwnd)
    if !o {
        TrayTip("没有记录该窗口的原始状态", APP)
        return
    }
    id := "ahk_id " hwnd
    DllCall("user32\SetWindowLongPtrW", "Ptr", hwnd, "Int", -16, "Ptr", o.style, "Ptr")
    WinSetAlwaysOnTop((o.ex & 0x8) ? 1 : 0, id)
    FrameChanged(hwnd)
    if o.HasOwnProp("x")
        WinMove(o.x, o.y, o.w, o.h, id)
    if gOriginal.Has(hwnd) {
        gOriginal.Delete(hwnd)
    }
    gTargetRect := []
    gGuard := false
    SetTimer(GuardTick, 0)
    SyncMenu()
    try FileDelete(StateFile)
    Log("restored")
    TrayTip("已还原原始样式与位置，并关闭保持", APP)
}

ToggleGuard() {
    global gGuard, APP
    gGuard := !gGuard
    SetTimer(GuardTick, gGuard ? 300 : 0)
    SyncMenu()
    TrayTip(gGuard ? "已开启：样式或全屏被改回时自动恢复" : "已关闭", APP)
}

GuardTick() {
    global gHwnd, gGuard, gTargetRect
    if !gHwnd || !WinExist("ahk_id " gHwnd) {
        gGuard := false
        SetTimer(GuardTick, 0)
        SyncMenu()
        return
    }
    FixStyle(gHwnd, false)
    if gTargetRect.Length = 4 {
        WinGetPos(&x, &y, &w, &h, "ahk_id " gHwnd)
        if (w >= A_ScreenWidth * 0.95 && h >= A_ScreenHeight * 0.85) {
            r := gTargetRect
            WinMove(r[1], r[2], r[3], r[4], "ahk_id " gHwnd)
            RaiseNoActivate(gHwnd)
            Log("guard: re-applied rect")
        }
    }
}

; 取窗口所在显示器的工作区，失败则回退主显示器工作区。
GetWorkArea(hwnd, &L, &T, &R, &B) {
    mi := Buffer(40, 0)                     ; MONITORINFO: cbSize + rcMonitor + rcWork + dwFlags
    NumPut("UInt", 40, mi, 0)
    hMon := DllCall("user32\MonitorFromWindow", "Ptr", hwnd, "UInt", 2, "Ptr")
    if hMon && DllCall("user32\GetMonitorInfoW", "Ptr", hMon, "Ptr", mi) {
        L := NumGet(mi, 20, "Int"), T := NumGet(mi, 24, "Int")
        R := NumGet(mi, 28, "Int"), B := NumGet(mi, 32, "Int")
        return true
    }
    rc := Buffer(16, 0)
    DllCall("user32\SystemParametersInfoW", "UInt", 0x30, "UInt", 0, "Ptr", rc, "UInt", 0)
    L := NumGet(rc, 0, "Int"), T := NumGet(rc, 4, "Int")
    R := NumGet(rc, 8, "Int"), B := NumGet(rc, 12, "Int")
    return false
}

ShowInfo() {
    global APP
    hwnd := PickTarget()
    if !hwnd {
        MsgBox("没有找到目标窗口")
        return
    }
    id := "ahk_id " hwnd
    GetWorkArea(hwnd, &L, &T, &R, &B)
    WinGetPos(&x, &y, &w, &h, id)
    MsgBox(
        "类名: " WinGetClass(id)
        "`n进程: " WinGetProcessName(id) "  (PID " WinGetPID(id) ")"
        "`nHWND: " hwnd
        "`nStyle: 0x" Format("{1:X}", WinGetStyle(id))
        "`nExStyle: 0x" Format("{1:X}", WinGetExStyle(id))
        "`n窗口矩形: " x ", " y "   " w " x " h
        "`n所在显示器工作区: " L ", " T " ~ " R ", " B
        , APP " - 目标窗口信息")
}

; ---------------- 状态持久化 ----------------
SaveState(hwnd, o) {
    global StateFile
    try {
        f := FileOpen(StateFile, "w")
        f.Write(hwnd "|" o.style "|" o.ex "|" o.x "," o.y "," o.w "," o.h "`n")
        f.Close()
    } catch {
    }
}

LoadState(hwnd) {
    global StateFile
    try {
        s := Trim(FileRead(StateFile), " `t`r`n")
    } catch {
        return 0
    }
    a := StrSplit(s, "|")
    if a.Length < 4 || Integer(Trim(a[1], " `t`r`n")) != hwnd
        return 0
    r := StrSplit(a[4], ",")
    if r.Length != 4
        return 0
    return {style: Integer(Trim(a[2], " `t`r`n")), ex: Integer(Trim(a[3], " `t`r`n")),
        x: Integer(Trim(r[1], " `t`r`n")), y: Integer(Trim(r[2], " `t`r`n")),
        w: Integer(Trim(r[3], " `t`r`n")), h: Integer(Trim(r[4], " `t`r`n"))}
}

; ---------------- 开机自启 / 提权 ----------------
ScriptCmd() {
    return A_IsCompiled ? '"' A_ScriptFullPath '"' : '"' A_AhkPath '" "' A_ScriptFullPath '"'
}

IsAutoStart() {
    global RunKey, APP
    try {
        return RegRead(RunKey, APP) != ""
    } catch {
        return false
    }
}

ToggleAutoStart() {
    global RunKey, APP
    if IsAutoStart() {
        try RegDelete(RunKey, APP)
    } else {
        RegWrite(ScriptCmd(), "REG_SZ", RunKey, APP)
    }
    SyncMenu()
    TrayTip(IsAutoStart() ? "已加入开机自启" : "已取消开机自启", APP)
}

RestartAsAdmin(*) {
    if A_IsCompiled
        Run('*RunAs "' A_ScriptFullPath '"')
    else
        Run('*RunAs "' A_AhkPath '" "' A_ScriptFullPath '"')
    ExitApp
}
