//! System tray icon and menu for background operation.

use crate::config::Config;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, SelectObject,
};
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_SEPARATOR,
    MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_USER,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY_CURRENT_USER,
    KEY_READ, KEY_WRITE, REG_SZ,
};

pub const WM_TRAY_ICON: u32 = WM_USER + 40;
pub const CMD_BROWSE: usize = 1001;
pub const CMD_OPEN_CONFIG: usize = 1002;
pub const CMD_AUTOSTART: usize = 1003;
pub const CMD_ELEVATE: usize = 1005;
pub const CMD_EXIT: usize = 1004;

pub struct TrayIcon {
    hwnd: HWND,
    hicon: HICON,
}

impl TrayIcon {
    pub fn create(hwnd: HWND) -> Self {
        let hicon = build_fathom_tray_icon();
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY_ICON,
            hIcon: hicon,
            ..Default::default()
        };
        let tip = "Winscroller Alt+Tab";
        for (dst, ch) in nid.szTip.iter_mut().zip(tip.encode_utf16()) {
            *dst = ch;
        }
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
        Self { hwnd, hicon }
    }

    pub fn show_menu(&self, is_elevated: bool) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return; };

            append_item(menu, CMD_BROWSE, "Open Winscroller Alt+Tab (Browse)", false);
            append_item(menu, CMD_OPEN_CONFIG, "Open config.toml", false);
            append_item(menu, CMD_AUTOSTART, "Start with Windows", is_autostart_enabled());
            if !is_elevated {
                append_item(menu, CMD_ELEVATE, "Restart as Administrator (for Admin apps)", false);
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            append_item(menu, CMD_EXIT, "Exit Winscroller Alt+Tab", false);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            if !self.hicon.0.is_null() {
                let _ = DestroyIcon(self.hicon);
            }
        }
    }
}

unsafe fn append_item(menu: windows::Win32::UI::WindowsAndMessaging::HMENU, id: usize, text: &str, checked: bool) {
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let flags = if checked { MF_STRING | MF_CHECKED } else { MF_STRING };
    let _ = AppendMenuW(menu, flags, id, PCWSTR(w.as_ptr()));
}

pub fn open_config_in_editor() {
    if let Some(path) = Config::ensure_default_file() {
        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let file: Vec<u16> = path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(file.as_ptr()),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "WinscrollerAltTab";

pub fn is_autostart_enabled() -> bool {
    let sub: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = RUN_VALUE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let ok = RegQueryValueExW(hkey, PCWSTR(val.as_ptr()), None, None, None, None).is_ok();
        let _ = RegCloseKey(hkey);
        ok
    }
}

pub fn toggle_autostart() {
    let sub: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = RUN_VALUE.encode_utf16().chain(std::iter::once(0)).collect();
    let enable = !is_autostart_enabled();
    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, KEY_WRITE, &mut hkey).is_err() {
            return;
        }
        if enable {
            if let Ok(exe) = std::env::current_exe() {
                let cmd = format!("\"{}\"", exe.to_string_lossy());
                let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                let _ = RegSetValueExW(hkey, PCWSTR(val.as_ptr()), 0, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(val.as_ptr()));
        }
        let _ = RegCloseKey(hkey);
    }
}

fn build_fathom_tray_icon() -> HICON {
    // 32x32 procedural icon: Tokyo Night diamond + concentric accent ring
    let size = 32usize;
    let mut pixels = vec![0u32; size * size];
    let cx = 15.5f32;
    let cy = 15.5f32;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - cx).abs();
            let dy = (y as f32 - cy).abs();
            let dist_diamond = dx + dy;
            let dist_box = dx.max(dy);
            if dist_diamond <= 7.2 {
                // #7aa2f7 accent diamond in BGRA
                pixels[y * size + x] = 0xFF_7A_A2_F7;
            } else if dist_box <= 13.5 && dist_box >= 11.2 {
                pixels[y * size + x] = 0xFF_A9_B1_D6;
            } else if dist_box < 11.2 {
                pixels[y * size + x] = 0xEE_1A_1B_26;
            }
        }
    }

    unsafe {
        let color_bmp = CreateBitmap(size as i32, size as i32, 1, 32, Some(pixels.as_ptr() as *const _));
        let mask_dc = CreateCompatibleDC(None);
        let mask_bmp = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let old = SelectObject(mask_dc, mask_bmp);
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        SelectObject(mask_dc, old);
        let _ = DeleteDC(mask_dc);
        let _ = DeleteObject(mask_bmp);
        let _ = DeleteObject(color_bmp);
        icon
    }
}
