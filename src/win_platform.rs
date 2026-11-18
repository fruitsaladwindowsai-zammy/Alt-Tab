//! Windows OS integration: window enumeration, Virtual Desktops, monitors, icons, wallpaper, and focusing.

use crate::controller::MonitorDesc;
use crate::field::WorkspaceDescriptor;
use crate::layout::Rect;
use crate::palette::{Rgba, ThemeSource};
use crate::recency::WindowCandidate;
use std::collections::HashSet;
use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use windows::core::{IUnknown, Interface, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWM_CLOAKED_APP,
    DWM_CLOAKED_INHERITED, DWM_CLOAKED_SHELL,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HDC, HMONITOR,
    MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_BINARY,
    REG_DWORD, REG_SZ,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
    OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LEFT, VK_LWIN, VK_RIGHT,
};
use windows::Win32::UI::Shell::{
    IVirtualDesktopManager, SHGetFileInfoW, ShellExecuteW, VirtualDesktopManager, SHFILEINFOW,
    SHGFI_ICON, SHGFI_LARGEICON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, EnumChildWindows, EnumWindows, GetAncestor,
    GetClassLongPtrW, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SendMessageTimeoutW,
    SetForegroundWindow, ShowWindow, SystemParametersInfoW, ASFW_ANY, GA_ROOTOWNER, GCLP_HICON,
    GCLP_HICONSM, GW_OWNER, HICON, ICON_BIG, ICON_SMALL2, SMTO_ABORTIFHUNG, SPI_GETDESKWALLPAPER,
    SW_RESTORE, SW_SHOWNORMAL, WINDOWPLACEMENT, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

pub fn enable_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn is_process_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

pub fn relaunch_elevated(extra_args: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let file: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_str = extra_args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let params: Vec<u16> = params_str
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            if params_str.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(params.as_ptr())
            },
            None,
            SW_SHOWNORMAL,
        )
    };
    (res.0 as usize) > 32
}

pub fn create_virtual_desktop_manager() -> Option<IVirtualDesktopManager> {
    unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok() }
}

pub fn read_windows_auto_theme() -> Option<ThemeSource> {
    let light = reg_read_dword(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "AppsUseLightTheme",
    )
    .map(|v| v != 0)
    .unwrap_or(false);

    let accent_abgr = reg_read_dword(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\DWM",
        "AccentColor",
    );
    let accent = accent_abgr.map(|abgr| {
        let r = (abgr & 0xFF) as u8;
        let g = ((abgr >> 8) & 0xFF) as u8;
        let b = ((abgr >> 16) & 0xFF) as u8;
        Rgba::from_u8(r, g, b, 255)
    });

    Some(crate::palette::windows_plain_theme(light, accent))
}

pub fn current_wallpaper_path() -> Option<String> {
    let mut buf = vec![0u16; 520];
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            Default::default(),
        )
        .is_ok()
    };
    if ok {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = OsString::from_wide(&buf[..len])
            .to_string_lossy()
            .into_owned();
        if !path.is_empty() && Path::new(&path).exists() {
            return Some(path);
        }
    }
    if let Some(path) = reg_read_sz(HKEY_CURRENT_USER, "Control Panel\\Desktop", "WallPaper") {
        if !path.is_empty() && Path::new(&path).exists() {
            return Some(path);
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let transcoded =
            Path::new(&appdata).join("Microsoft\\Windows\\Themes\\TranscodedWallpaper");
        if transcoded.exists() {
            return Some(transcoded.to_string_lossy().into_owned());
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct VirtualDesktopInfo {
    pub guid: GUID,
    pub index_one_based: usize,
    pub name: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, Default)]
pub struct VirtualDesktopSnapshot {
    pub ordered: Vec<VirtualDesktopInfo>,
    #[allow(dead_code)]
    pub current_guid: Option<GUID>,
}

impl VirtualDesktopSnapshot {
    pub fn read() -> Self {
        let base = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops";
        if let Some((com_guids, com_curr)) = com_list_desktops() {
            if !com_guids.is_empty() {
                let ordered = com_guids
                    .iter()
                    .enumerate()
                    .map(|(i, &guid)| {
                        Self::build_info(
                            base,
                            guid,
                            i + 1,
                            com_curr.map(|c| c == guid).unwrap_or(i == 0),
                        )
                    })
                    .collect();
                return Self {
                    ordered,
                    current_guid: com_curr,
                };
            }
        }

        let mut session_id = 0u32;
        unsafe {
            let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id);
        }
        let session_base = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\SessionInfo\\{session_id}\\VirtualDesktops"
        );

        let current_guid = reg_read_binary(HKEY_CURRENT_USER, base, "CurrentVirtualDesktop")
            .or_else(|| reg_read_binary(HKEY_CURRENT_USER, &session_base, "CurrentVirtualDesktop"))
            .and_then(|bytes| parse_guid_bytes(&bytes));

        let mut guids: Vec<GUID> = reg_read_binary(HKEY_CURRENT_USER, base, "VirtualDesktopIDs")
            .or_else(|| reg_read_binary(HKEY_CURRENT_USER, &session_base, "VirtualDesktopIDs"))
            .map(|bytes| {
                bytes
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .filter_map(|c| parse_guid_bytes(c))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(curr) = current_guid {
            if !guids.is_empty() && !guids.contains(&curr) {
                guids.insert(0, curr);
            }
        }

        let ordered = guids
            .iter()
            .enumerate()
            .map(|(i, &guid)| {
                Self::build_info(
                    base,
                    guid,
                    i + 1,
                    current_guid.map(|c| c == guid).unwrap_or(i == 0),
                )
            })
            .collect();

        Self {
            ordered,
            current_guid,
        }
    }

    fn build_info(
        base: &str,
        guid: GUID,
        index_one_based: usize,
        is_current: bool,
    ) -> VirtualDesktopInfo {
        let guid_str = format_guid_braced(guid);
        let custom_name = if guid != GUID::zeroed() {
            reg_read_sz(
                HKEY_CURRENT_USER,
                &format!("{base}\\Desktops\\{guid_str}"),
                "Name",
            )
            .filter(|s| !s.trim().is_empty())
        } else {
            None
        };
        VirtualDesktopInfo {
            guid,
            index_one_based,
            name: custom_name.unwrap_or_else(|| index_one_based.to_string()),
            is_current,
        }
    }

    pub fn index_of_key(&self, key: &str) -> Option<usize> {
        self.ordered
            .iter()
            .position(|d| format!("vd:{}", format_guid_braced(d.guid)) == key)
    }

    pub fn current_index(&self) -> Option<usize> {
        self.ordered.iter().position(|d| d.is_current)
    }
}

pub fn enumerate_monitors() -> (Vec<MonitorDesc>, Rect) {
    struct State {
        monitors: Vec<MonitorDesc>,
    }
    unsafe extern "system" fn callback(
        hmon: HMONITOR,
        _hdc: HDC,
        _rc: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let state = &mut *(lparam.0 as *mut State);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut _).as_bool() {
            let r = info.monitorInfo.rcMonitor;
            let len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            let raw_name = OsString::from_wide(&info.szDevice[..len])
                .to_string_lossy()
                .into_owned();
            let short = raw_name.trim_start_matches("\\\\.\\").to_string();
            state.monitors.push(MonitorDesc {
                name: short,
                rect: Rect {
                    x: r.left as f64,
                    y: r.top as f64,
                    width: (r.right - r.left).max(1) as f64,
                    height: (r.bottom - r.top).max(1) as f64,
                },
                focused: (info.monitorInfo.dwFlags & 1) != 0,
            });
        }
        BOOL(1)
    }

    let mut state = State {
        monitors: Vec::new(),
    };
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(callback),
            LPARAM(&mut state as *mut _ as isize),
        );
    }

    let target_rect = unsafe {
        let fg = GetForegroundWindow();
        let hmon = if !fg.0.is_null() {
            MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST)
        } else {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
        };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut _).as_bool() {
            let r = info.monitorInfo.rcMonitor;
            let len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            let focused_name = OsString::from_wide(&info.szDevice[..len])
                .to_string_lossy()
                .trim_start_matches("\\\\.\\")
                .to_string();
            for m in &mut state.monitors {
                m.focused = m.name == focused_name;
            }
            Rect {
                x: r.left as f64,
                y: r.top as f64,
                width: (r.right - r.left).max(1) as f64,
                height: (r.bottom - r.top).max(1) as f64,
            }
        } else {
            state.monitors.first().map(|m| m.rect).unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            })
        }
    };

    (state.monitors, target_rect)
}

pub fn enumerate_windows(
    overlay_hwnd: HWND,
    vdm: Option<&IVirtualDesktopManager>,
    vd_snapshot: &VirtualDesktopSnapshot,
    monitors: &[MonitorDesc],
    urgent_hwnds: &HashSet<isize>,
) -> (
    Vec<WindowCandidate>,
    Vec<WorkspaceDescriptor>,
    Option<isize>,
) {
    struct EnumState {
        overlay: isize,
        raw_hwnds: Vec<HWND>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);
        if hwnd.0 as isize != state.overlay {
            state.raw_hwnds.push(hwnd);
        }
        BOOL(1)
    }

    let mut state = EnumState {
        overlay: overlay_hwnd.0 as isize,
        raw_hwnds: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));
    }

    let fg_hwnd = unsafe { GetForegroundWindow() };
    let fg_id = if fg_hwnd.0.is_null() || fg_hwnd == overlay_hwnd {
        None
    } else {
        Some(fg_hwnd.0 as isize)
    };

    struct RawWin {
        hwnd: HWND,
        pid: u32,
        title: String,
        app_id: String,
        app_name: String,
        exe_path: String,
        geometry: Option<Rect>,
        monitor_name: String,
        minimized: bool,
        on_current_desktop: bool,
        desktop_guid: Option<GUID>,
        z_rank: usize,
    }

    let mut raw_list = Vec::new();
    let mut z_rank = 0usize;

    for hwnd in state.raw_hwnds {
        if !is_alt_tab_window(hwnd, vdm) {
            continue;
        }
        let title = get_window_title(hwnd);
        if title.is_empty() {
            continue;
        }
        let class_name = get_class_name(hwnd);
        let (pid, exe_path) = resolve_window_process(hwnd, &class_name);
        let exe_stem = Path::new(&exe_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&class_name)
            .to_string();
        let friendly = if class_name == "ApplicationFrameWindow" {
            title.split(" - ").last().unwrap_or(&title).to_string()
        } else {
            String::new()
        };
        let app_name = crate::field::clean_app_name(&exe_stem, &friendly);

        let minimized = unsafe { IsIconic(hwnd).as_bool() };
        let geometry = window_geometry(hwnd, minimized);
        let monitor_name = monitor_for_hwnd(hwnd, monitors);

        let on_current_desktop = vdm
            .and_then(|m| unsafe { m.IsWindowOnCurrentVirtualDesktop(hwnd).ok() })
            .map(|b| b.as_bool())
            .unwrap_or(true);

        let desktop_guid = vdm
            .and_then(|m| unsafe { m.GetWindowDesktopId(hwnd).ok() })
            .filter(|g| *g != GUID::zeroed());

        raw_list.push(RawWin {
            hwnd,
            pid,
            title,
            app_id: exe_stem,
            app_name,
            exe_path,
            geometry,
            monitor_name,
            minimized,
            on_current_desktop,
            desktop_guid,
            z_rank,
        });
        z_rank += 1;
    }

    // Reconcile the authoritative ordered list of active Virtual Desktops.
    let vd_base = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops";
    let mut active_guid = vd_snapshot.current_guid.filter(|g| *g != GUID::zeroed());
    if active_guid.is_none() {
        active_guid = raw_list
            .iter()
            .find(|w| w.on_current_desktop && w.desktop_guid.is_some())
            .and_then(|w| w.desktop_guid);
    }

    let mut ordered_desktops: Vec<VirtualDesktopInfo> = if !vd_snapshot.ordered.is_empty() {
        vd_snapshot
            .ordered
            .iter()
            .map(|d| VirtualDesktopInfo {
                is_current: active_guid.map(|a| a == d.guid).unwrap_or(d.is_current),
                ..d.clone()
            })
            .collect()
    } else {
        let mut guids: Vec<GUID> = Vec::new();
        if let Some(ag) = active_guid {
            guids.push(ag);
        }
        for w in &raw_list {
            if let Some(g) = w.desktop_guid {
                if !guids.contains(&g) {
                    guids.push(g);
                }
            }
        }
        if guids.is_empty() {
            guids.push(GUID::zeroed());
        }
        guids
            .iter()
            .enumerate()
            .map(|(i, &g)| {
                let is_curr = active_guid.map(|a| a == g).unwrap_or(i == 0);
                VirtualDesktopSnapshot::build_info(vd_base, g, i + 1, is_curr)
            })
            .collect()
    };

    if !ordered_desktops.iter().any(|d| d.is_current) {
        if let Some(first) = ordered_desktops.first_mut() {
            first.is_current = true;
        }
    }

    // Ensure any off-current desktop GUID seen on a real window exists in ordered_desktops.
    for w in &raw_list {
        if !w.on_current_desktop {
            if let Some(g) = w.desktop_guid {
                if !ordered_desktops.iter().any(|d| d.guid == g) {
                    let idx = ordered_desktops.len() + 1;
                    ordered_desktops
                        .push(VirtualDesktopSnapshot::build_info(vd_base, g, idx, false));
                }
            }
        }
    }

    let mut candidates = Vec::with_capacity(raw_list.len());
    for w in raw_list {
        let info = w
            .desktop_guid
            .and_then(|g| ordered_desktops.iter().find(|d| d.guid == g))
            .or_else(|| {
                if w.on_current_desktop {
                    ordered_desktops.iter().find(|d| d.is_current)
                } else {
                    None
                }
            })
            .unwrap_or(&ordered_desktops[0]);

        let workspace_key = if info.guid != GUID::zeroed() {
            format!("vd:{}", format_guid_braced(info.guid))
        } else {
            "vd:1".to_string()
        };

        candidates.push(WindowCandidate {
            hwnd: w.hwnd.0 as isize,
            pid: w.pid,
            title: w.title,
            app_id: w.app_id,
            app_name: w.app_name,
            exe_path: w.exe_path,
            workspace_key,
            workspace_id: Some(info.index_one_based as i64),
            workspace_name: info.name.clone(),
            monitor_name: w.monitor_name,
            geometry: w.geometry,
            minimized: w.minimized,
            on_current_desktop: w.on_current_desktop,
            urgent: urgent_hwnds.contains(&(w.hwnd.0 as isize)),
            z_rank: w.z_rank,
        });
    }

    let focused_monitor = monitors
        .iter()
        .find(|m| m.focused)
        .or_else(|| monitors.first())
        .map(|m| m.name.clone())
        .unwrap_or_default();

    let workspaces: Vec<WorkspaceDescriptor> = ordered_desktops
        .iter()
        .map(|d| WorkspaceDescriptor {
            key: if d.guid != GUID::zeroed() {
                format!("vd:{}", format_guid_braced(d.guid))
            } else {
                "vd:1".to_string()
            },
            id: Some(d.index_one_based as i64),
            name: d.name.clone(),
            monitor: focused_monitor.clone(),
            on_screen: d.is_current,
        })
        .collect();

    (candidates, workspaces, fg_id)
}

pub fn is_alt_tab_window(hwnd: HWND, vdm: Option<&IVirtualDesktopManager>) -> bool {
    unsafe {
        if !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let ex_style =
            GetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 && (ex_style & WS_EX_APPWINDOW.0) == 0 {
            return false;
        }
        if (ex_style & WS_EX_NOACTIVATE.0) != 0 && (ex_style & WS_EX_APPWINDOW.0) == 0 {
            return false;
        }

        // Standard Alt+Tab owner chain walk: exclude owned popups unless APPWINDOW.
        if (ex_style & WS_EX_APPWINDOW.0) == 0 {
            let owner = GetWindow(hwnd, GW_OWNER);
            if owner.is_ok_and(|o| !o.0.is_null()) {
                return false;
            }
            let root_owner = GetAncestor(hwnd, GA_ROOTOWNER);
            if root_owner != hwnd
                && !root_owner.0.is_null()
                && IsWindowVisible(root_owner).as_bool()
            {
                return false;
            }
        }

        let class_name = get_class_name(hwnd);
        if matches!(
            class_name.as_str(),
            "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "Progman"
                | "WorkerW"
                | "DV2ControlHost"
                | "Windows.UI.Core.CoreWindow"
                | "TopLevelWindowForOverflowXamlIsland"
                | "Xaml_WindowedPopupClass"
                | "EdgeUiInputTopWndClass"
                | "NativeHWNDHost"
        ) {
            return false;
        }

        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            // Keep windows cloaked only by shell virtual-desktop switching when they actually belong
            // to a non-current virtual desktop; filter out suspended UWP ghosts on the current desktop.
            if (cloaked & (DWM_CLOAKED_APP | DWM_CLOAKED_INHERITED)) != 0 {
                return false;
            }
            if (cloaked & DWM_CLOAKED_SHELL) != 0 {
                let Some(mgr) = vdm else { return false };
                let on_current = mgr
                    .IsWindowOnCurrentVirtualDesktop(hwnd)
                    .map(|b| b.as_bool())
                    .unwrap_or(true);
                if on_current {
                    return false;
                }
                let guid = mgr.GetWindowDesktopId(hwnd).unwrap_or_default();
                if guid == GUID::zeroed() {
                    return false;
                }
            }
        }

        true
    }
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 2];
        let actual = GetWindowTextW(hwnd, &mut buf);
        OsString::from_wide(&buf[..actual as usize])
            .to_string_lossy()
            .trim()
            .to_string()
    }
}

pub fn get_class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .into_owned()
    }
}

fn process_image_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        if ok && len > 0 {
            Some(
                OsString::from_wide(&buf[..len as usize])
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        }
    }
}

fn resolve_window_process(hwnd: HWND, class_name: &str) -> (u32, String) {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let mut path = process_image_path(pid).unwrap_or_default();

    // UWP windows are hosted inside ApplicationFrameHost.exe; inspect the child CoreWindow for the real app.
    if class_name == "ApplicationFrameWindow"
        || path
            .to_ascii_lowercase()
            .ends_with("applicationframehost.exe")
    {
        struct ChildPid {
            host_pid: u32,
            real_pid: Option<u32>,
        }
        unsafe extern "system" fn find_child(child: HWND, lparam: LPARAM) -> BOOL {
            let state = &mut *(lparam.0 as *mut ChildPid);
            let mut cpid: u32 = 0;
            GetWindowThreadProcessId(child, Some(&mut cpid));
            if cpid != 0 && cpid != state.host_pid {
                state.real_pid = Some(cpid);
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut state = ChildPid {
            host_pid: pid,
            real_pid: None,
        };
        unsafe {
            let _ = EnumChildWindows(
                hwnd,
                Some(find_child),
                LPARAM(&mut state as *mut _ as isize),
            );
        }
        if let Some(real_pid) = state.real_pid {
            if let Some(real_path) = process_image_path(real_pid) {
                pid = real_pid;
                path = real_path;
            }
        }
    }

    (pid, path)
}

fn window_geometry(hwnd: HWND, minimized: bool) -> Option<Rect> {
    unsafe {
        if minimized {
            let mut wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            if GetWindowPlacement(hwnd, &mut wp).is_ok() {
                let r = wp.rcNormalPosition;
                if r.right > r.left && r.bottom > r.top && r.left > -20000 && r.top > -20000 {
                    return Some(Rect {
                        x: r.left as f64,
                        y: r.top as f64,
                        width: (r.right - r.left) as f64,
                        height: (r.bottom - r.top) as f64,
                    });
                }
            }
            return None;
        }
        let mut r = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok()
            && r.right > r.left
            && r.bottom > r.top
            && r.left > -20000
            && r.top > -20000
        {
            return Some(Rect {
                x: r.left as f64,
                y: r.top as f64,
                width: (r.right - r.left) as f64,
                height: (r.bottom - r.top) as f64,
            });
        }
        if GetWindowRect(hwnd, &mut r).is_ok()
            && r.right > r.left
            && r.bottom > r.top
            && r.left > -20000
            && r.top > -20000
        {
            return Some(Rect {
                x: r.left as f64,
                y: r.top as f64,
                width: (r.right - r.left) as f64,
                height: (r.bottom - r.top) as f64,
            });
        }
    }
    None
}

fn monitor_for_hwnd(hwnd: HWND, monitors: &[MonitorDesc]) -> String {
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut _).as_bool() {
            let len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            return OsString::from_wide(&info.szDevice[..len])
                .to_string_lossy()
                .trim_start_matches("\\\\.\\")
                .to_string();
        }
    }
    monitors.first().map(|m| m.name.clone()).unwrap_or_default()
}

pub fn extract_window_hicon(hwnd: HWND, exe_path: &str) -> Option<(HICON, bool)> {
    unsafe {
        let mut icon_ptr: usize = 0;
        let ok = SendMessageTimeoutW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_GETICON,
            WPARAM(ICON_BIG as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            30,
            Some(&mut icon_ptr),
        );
        if ok.0 != 0 && icon_ptr != 0 {
            return Some((HICON(icon_ptr as *mut _), false));
        }

        let class_icon = GetClassLongPtrW(hwnd, GCLP_HICON);
        if class_icon != 0 {
            return Some((HICON(class_icon as *mut _), false));
        }

        let ok_sm = SendMessageTimeoutW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_GETICON,
            WPARAM(ICON_SMALL2 as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            25,
            Some(&mut icon_ptr),
        );
        if ok_sm.0 != 0 && icon_ptr != 0 {
            return Some((HICON(icon_ptr as *mut _), false));
        }

        let class_sm = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        if class_sm != 0 {
            return Some((HICON(class_sm as *mut _), false));
        }

        if !exe_path.is_empty() {
            let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut shfi = SHFILEINFOW::default();
            let res = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                Default::default(),
                Some(&mut shfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if res != 0 && !shfi.hIcon.0.is_null() {
                return Some((shfi.hIcon, true));
            }
        }
    }
    None
}

pub fn focus_target_window(
    hwnd_raw: isize,
    on_current_desktop: bool,
    target_workspace_key: &str,
    minimized: bool,
) {
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return;
        }
        if !on_current_desktop {
            let vd = VirtualDesktopSnapshot::read();
            if let (Some(curr), Some(target)) =
                (vd.current_index(), vd.index_of_key(target_workspace_key))
            {
                let delta = target as isize - curr as isize;
                if delta != 0 {
                    switch_virtual_desktop_steps(delta);
                    std::thread::sleep(std::time::Duration::from_millis(65));
                }
            }
        }

        if minimized || IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let fg = GetForegroundWindow();
        let fg_tid = if !fg.0.is_null() {
            GetWindowThreadProcessId(fg, None)
        } else {
            0
        };
        let my_tid = GetCurrentThreadId();
        let attached =
            fg_tid != 0 && fg_tid != my_tid && AttachThreadInput(my_tid, fg_tid, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(my_tid, fg_tid, false);
        }
    }
}

fn switch_virtual_desktop_steps(delta: isize) {
    let arrow = if delta > 0 { VK_RIGHT } else { VK_LEFT };
    for _ in 0..delta.abs() {
        let inputs = [
            key_input(VK_LWIN, false, true),
            key_input(VK_CONTROL, false, false),
            key_input(arrow, false, true),
            key_input(arrow, true, true),
            key_input(VK_CONTROL, true, false),
            key_input(VK_LWIN, true, true),
        ];
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

pub fn switch_virtual_desktop_by_key(target_workspace_key: &str) {
    let vd = VirtualDesktopSnapshot::read();
    if let (Some(curr), Some(target)) = (vd.current_index(), vd.index_of_key(target_workspace_key))
    {
        let delta = target as isize - curr as isize;
        if delta != 0 {
            switch_virtual_desktop_steps(delta);
        }
    }
}

pub fn create_virtual_desktop(
    overlay_hwnd: HWND,
    move_hwnd: Option<isize>,
    vdm: Option<&IVirtualDesktopManager>,
) -> Option<VirtualDesktopInfo> {
    let before = VirtualDesktopSnapshot::read();
    let old_curr_idx = before.current_index().unwrap_or(0);
    let old_guids: Vec<GUID> = before.ordered.iter().map(|d| d.guid).collect();

    const VK_D: VIRTUAL_KEY = VIRTUAL_KEY(0x44);
    let inputs = [
        key_input(VK_LWIN, false, true),
        key_input(VK_CONTROL, false, false),
        key_input(VK_D, false, false),
        key_input(VK_D, true, false),
        key_input(VK_CONTROL, true, false),
        key_input(VK_LWIN, true, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }

    let mut after = before.clone();
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        let snap = VirtualDesktopSnapshot::read();
        if snap.ordered.len() > before.ordered.len() {
            after = snap;
            break;
        }
    }

    if after.ordered.len() <= before.ordered.len() {
        force_focus_overlay(overlay_hwnd);
        return None;
    }

    let new_info = after
        .ordered
        .iter()
        .find(|d| !old_guids.contains(&d.guid))
        .or_else(|| after.ordered.last())
        .cloned()?;

    if let Some(hwnd_raw) = move_hwnd {
        let _ = move_window_to_desktop_guid(hwnd_raw, new_info.guid, vdm);
    }

    let latest = VirtualDesktopSnapshot::read();
    if let Some(curr_after) = latest.current_index() {
        let delta = old_curr_idx as isize - curr_after as isize;
        if delta != 0 {
            switch_virtual_desktop_steps(delta);
            std::thread::sleep(std::time::Duration::from_millis(55));
        }
    }
    force_focus_overlay(overlay_hwnd);
    Some(new_info)
}

pub fn move_window_to_desktop_key(
    hwnd_raw: isize,
    target_workspace_key: &str,
    vdm: Option<&IVirtualDesktopManager>,
) -> bool {
    let target_guid = parse_workspace_key_guid(target_workspace_key).or_else(|| {
        let vd = VirtualDesktopSnapshot::read();
        vd.index_of_key(target_workspace_key)
            .and_then(|idx| vd.ordered.get(idx))
            .map(|d| d.guid)
    });
    let Some(guid) = target_guid else {
        return false;
    };
    move_window_to_desktop_guid(hwnd_raw, guid, vdm)
}

fn parse_workspace_key_guid(key: &str) -> Option<GUID> {
    let hex_part = key.strip_prefix("vd:")?;
    parse_guid_str(hex_part)
}

fn parse_guid_str(s: &str) -> Option<GUID> {
    let clean: String = s
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .chars()
        .filter(|c| *c != '-')
        .collect();
    if clean.len() != 32 {
        return None;
    }
    let d1 = u32::from_str_radix(&clean[0..8], 16).ok()?;
    let d2 = u16::from_str_radix(&clean[8..12], 16).ok()?;
    let d3 = u16::from_str_radix(&clean[12..16], 16).ok()?;
    let mut d4 = [0u8; 8];
    for (i, slot) in d4.iter_mut().enumerate() {
        let start = 16 + i * 2;
        *slot = u8::from_str_radix(&clean[start..start + 2], 16).ok()?;
    }
    Some(GUID::from_values(d1, d2, d3, d4))
}

fn key_input(vk: VIRTUAL_KEY, keyup: bool, extended: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if keyup {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn force_focus_overlay(hwnd: HWND) {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let fg = GetForegroundWindow();
        let fg_tid = if !fg.0.is_null() && fg != hwnd {
            GetWindowThreadProcessId(fg, None)
        } else {
            0
        };
        let my_tid = GetCurrentThreadId();
        let attached =
            fg_tid != 0 && fg_tid != my_tid && AttachThreadInput(my_tid, fg_tid, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(my_tid, fg_tid, false);
        }
    }
}

pub fn reg_read_dword(root: HKEY, subkey: &str, value_name: &str) -> Option<u32> {
    let sub: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut kind = Default::default();
        let ok = RegQueryValueExW(
            hkey,
            PCWSTR(val.as_ptr()),
            None,
            Some(&mut kind),
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        if ok && kind == REG_DWORD {
            Some(data)
        } else {
            None
        }
    }
}

pub fn reg_read_sz(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    let sub: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut size = 0u32;
        let mut kind = Default::default();
        if RegQueryValueExW(
            hkey,
            PCWSTR(val.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
        .is_err()
            || size == 0
            || kind != REG_SZ
        {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let ok = RegQueryValueExW(
            hkey,
            PCWSTR(val.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        if !ok {
            return None;
        }
        let wide: Vec<u16> = buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&c| c != 0)
            .collect();
        Some(OsString::from_wide(&wide).to_string_lossy().into_owned())
    }
}

pub fn reg_read_binary(root: HKEY, subkey: &str, value_name: &str) -> Option<Vec<u8>> {
    let sub: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut size = 0u32;
        let mut kind = Default::default();
        if RegQueryValueExW(
            hkey,
            PCWSTR(val.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
        .is_err()
            || size == 0
            || kind != REG_BINARY
        {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let ok = RegQueryValueExW(
            hkey,
            PCWSTR(val.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        if ok {
            Some(buf)
        } else {
            None
        }
    }
}

fn parse_guid_bytes(bytes: &[u8]) -> Option<GUID> {
    if bytes.len() < 16 {
        return None;
    }
    Some(GUID {
        data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        data2: u16::from_le_bytes([bytes[4], bytes[5]]),
        data3: u16::from_le_bytes([bytes[6], bytes[7]]),
        data4: [
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ],
    })
}

pub fn format_guid_braced(g: GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7],
    )
}

// --- Windows 11 internal Virtual Desktop COM helpers (cross-process safe) ---

const CLSID_IMMERSIVE_SHELL: GUID = GUID::from_values(
    0xC2F03A33,
    0x21F5,
    0x47FA,
    [0xB4, 0xBB, 0x15, 0x63, 0x62, 0xA2, 0xF2, 0x39],
);
const CLSID_VDM_INTERNAL: GUID = GUID::from_values(
    0xC5E0CDCA,
    0x7B6E,
    0x41B2,
    [0x9F, 0xC4, 0xD9, 0x39, 0x75, 0xCC, 0x46, 0x7B],
);
const IID_VDM_INTERNAL_24H2: GUID = GUID::from_values(
    0x53F5CA0B,
    0x158F,
    0x4124,
    [0x90, 0x0C, 0x05, 0x71, 0x58, 0x06, 0x0B, 0x27],
);
const IID_VDM_INTERNAL_23H2: GUID = GUID::from_values(
    0xA3175F2D,
    0x239C,
    0x4BD2,
    [0x8A, 0xA0, 0xEE, 0xBA, 0x8B, 0x0B, 0x13, 0x8E],
);
const IID_IVIRTUAL_DESKTOP_24H2: GUID = GUID::from_values(
    0x3F07F4BE,
    0xB107,
    0x441A,
    [0xAF, 0x0F, 0x39, 0xD8, 0x25, 0x29, 0x07, 0x2C],
);
const IID_IVIRTUAL_DESKTOP_ALT: GUID = GUID::from_values(
    0x536D3495,
    0xB208,
    0x4CC9,
    [0xAE, 0x26, 0xDE, 0x81, 0x11, 0x27, 0x5B, 0xF8],
);

const IID_ISERVICE_PROVIDER: GUID = GUID::from_values(
    0x6D5140C1,
    0x7436,
    0x11CE,
    [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
);
const IID_IAPPLICATION_VIEW_COLLECTION: GUID = GUID::from_values(
    0x1841C6D7,
    0x4F9D,
    0x42C0,
    [0xAF, 0x41, 0x87, 0x47, 0x53, 0x8F, 0x10, 0xE5],
);

#[inline]
unsafe fn vtable_fn(obj: &IUnknown, slot: usize) -> *const c_void {
    let raw = obj.as_raw();
    let vt = *(raw as *const *const *const c_void);
    *vt.add(slot)
}

unsafe fn sp_query_service(sp: &IUnknown, service: &GUID, iid: &GUID) -> Option<IUnknown> {
    let f: unsafe extern "system" fn(
        *mut c_void,
        *const GUID,
        *const GUID,
        *mut *mut c_void,
    ) -> HRESULT = std::mem::transmute(vtable_fn(sp, 3));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(sp.as_raw(), service, iid, &mut out).is_ok() && !out.is_null() {
        Some(IUnknown::from_raw(out))
    } else {
        None
    }
}

unsafe fn obj_array_count(arr: &IUnknown) -> Option<u32> {
    let f: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT =
        std::mem::transmute(vtable_fn(arr, 3));
    let mut count = 0u32;
    if f(arr.as_raw(), &mut count).is_ok() {
        Some(count)
    } else {
        None
    }
}

unsafe fn obj_array_at(arr: &IUnknown, index: u32, iid: &GUID) -> Option<IUnknown> {
    let f: unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void) -> HRESULT =
        std::mem::transmute(vtable_fn(arr, 4));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(arr.as_raw(), index, iid, &mut out).is_ok() && !out.is_null() {
        Some(IUnknown::from_raw(out))
    } else {
        None
    }
}

unsafe fn vd_get_id(vd: &IUnknown) -> Option<GUID> {
    // Slot 4 on IVirtualDesktop (`3F07F4BE-...` and `536D3495-...`) is GetId(GUID*)
    let f: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT =
        std::mem::transmute(vtable_fn(vd, 4));
    let mut guid = GUID::zeroed();
    if f(vd.as_raw(), &mut guid).is_ok() && guid != GUID::zeroed() {
        Some(guid)
    } else {
        None
    }
}

unsafe fn vdm_get_current_desktop(vdm: &IUnknown) -> Option<IUnknown> {
    let f: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT =
        std::mem::transmute(vtable_fn(vdm, 6));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(vdm.as_raw(), &mut out).is_ok() && !out.is_null() {
        Some(IUnknown::from_raw(out))
    } else {
        None
    }
}

unsafe fn vdm_get_desktops(vdm: &IUnknown) -> Option<IUnknown> {
    let f: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT =
        std::mem::transmute(vtable_fn(vdm, 7));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(vdm.as_raw(), &mut out).is_ok() && !out.is_null() {
        Some(IUnknown::from_raw(out))
    } else {
        None
    }
}

unsafe fn vdm_move_view_to_desktop(vdm: &IUnknown, view: &IUnknown, desktop: &IUnknown) -> bool {
    let f: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT =
        std::mem::transmute(vtable_fn(vdm, 4));
    f(vdm.as_raw(), view.as_raw(), desktop.as_raw()).is_ok()
}

unsafe fn avc_get_view_for_hwnd(avc: &IUnknown, hwnd: HWND) -> Option<IUnknown> {
    // Slot 6 on IApplicationViewCollection is GetViewForHwnd(HWND, IApplicationView**)
    let f: unsafe extern "system" fn(*mut c_void, HWND, *mut *mut c_void) -> HRESULT =
        std::mem::transmute(vtable_fn(avc, 6));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(avc.as_raw(), hwnd, &mut out).is_ok() && !out.is_null() {
        Some(IUnknown::from_raw(out))
    } else {
        None
    }
}

fn query_internal_vdm() -> Option<(IUnknown, IUnknown)> {
    unsafe {
        let shell_unk: IUnknown =
            CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_ALL).ok()?;
        let mut sp_raw: *mut c_void = std::ptr::null_mut();
        if shell_unk
            .query(&IID_ISERVICE_PROVIDER, &mut sp_raw)
            .is_err()
            || sp_raw.is_null()
        {
            return None;
        }
        let sp = IUnknown::from_raw(sp_raw);
        for iid in [IID_VDM_INTERNAL_24H2, IID_VDM_INTERNAL_23H2] {
            if let Some(vdm) = sp_query_service(&sp, &CLSID_VDM_INTERNAL, &iid) {
                return Some((sp, vdm));
            }
        }
        None
    }
}

fn read_desktop_guid(arr: &IUnknown, index: u32) -> Option<(GUID, IUnknown)> {
    unsafe {
        for vd_iid in [IID_IVIRTUAL_DESKTOP_24H2, IID_IVIRTUAL_DESKTOP_ALT] {
            if let Some(vd) = obj_array_at(arr, index, &vd_iid) {
                if let Some(g) = vd_get_id(&vd) {
                    return Some((g, vd));
                }
            }
        }
        None
    }
}

fn com_list_desktops() -> Option<(Vec<GUID>, Option<GUID>)> {
    let (_sp, vdm) = query_internal_vdm()?;
    unsafe {
        let curr_guid = vdm_get_current_desktop(&vdm).and_then(|vd| vd_get_id(&vd));
        let arr = vdm_get_desktops(&vdm)?;
        let count = obj_array_count(&arr)?;
        let mut guids = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some((g, _vd)) = read_desktop_guid(&arr, i) {
                guids.push(g);
            }
        }
        Some((guids, curr_guid))
    }
}

fn move_window_to_desktop_guid(
    hwnd_raw: isize,
    target_guid: GUID,
    vdm: Option<&IVirtualDesktopManager>,
) -> bool {
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return false;
        }

        if let Some((sp, internal)) = query_internal_vdm() {
            if let Some(avc) = sp_query_service(
                &sp,
                &IID_IAPPLICATION_VIEW_COLLECTION,
                &IID_IAPPLICATION_VIEW_COLLECTION,
            ) {
                if let Some(view) = avc_get_view_for_hwnd(&avc, hwnd) {
                    if let Some(arr) = vdm_get_desktops(&internal) {
                        if let Some(count) = obj_array_count(&arr) {
                            for i in 0..count {
                                if let Some((g, desktop_vd)) = read_desktop_guid(&arr, i) {
                                    if g == target_guid
                                        && vdm_move_view_to_desktop(&internal, &view, &desktop_vd)
                                    {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(m) = vdm {
            if m.MoveWindowToDesktop(hwnd, &target_guid).is_ok() {
                return true;
            }
        }
    }
    false
}
