//! Winscroller Alt+Tab — Depth-based Alt+Tab window switcher and workspace overview for Windows.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod controller;
mod demo;
mod depth;
mod field;
mod layout;
mod palette;
mod recency;
mod renderer;
mod tray;
mod win_capture;
mod win_hook;
mod win_platform;

use config::Config;
use controller::{now_epoch_ms, Action, Controller, SwitchMode};
use layout::Rect;
use renderer::{HitRegions, Renderer};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tray::TrayIcon;
use win_capture::{CaptureService, MediaStore, WM_FATHOM_CAPTURE_READY, WM_FATHOM_WALLPAPER_READY};
use win_hook::{
    HooksGuard, WM_FATHOM_ALT_RELEASE, WM_FATHOM_CHORD_STEP, WM_FATHOM_FOCUS_CHANGED,
    WM_FATHOM_OPEN_BROWSE, WM_FATHOM_PRE_REVEAL_KEY, WM_FATHOM_WINDOW_CLOSED,
    WM_FATHOM_WINDOW_OPENED,
};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, ToUnicode, VK_BACK, VK_CONTROL, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::Shell::IVirtualDesktopManager;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, KillTimer,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterShellHookWindow, RegisterWindowMessageW,
    SetTimer, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST,
    MSG, SC_KEYMENU, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, WM_ACTIVATE, WM_CHAR,
    WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEHWHEEL,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONUP, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_TIMER,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const HSHELL_FLASH: usize = 0x8006;

const WINDOW_CLASS: PCWSTR = w!("WinscrollerAltTabOverlayClass");
const TIMER_TICK_ID: usize = 1;
const TIMER_REVEAL_ID: usize = 2;
const WM_FATHOM_QUIT: u32 = 0x8000 + 99;
const ERROR_ALREADY_EXISTS: u32 = 183;

struct AppState {
    hwnd: HWND,
    ctrl: Controller,
    renderer: Renderer,
    capture: CaptureService,
    wic: Option<IWICImagingFactory>,
    vdm: Option<IVirtualDesktopManager>,
    tray: Option<TrayIcon>,
    urgent_hwnds: HashSet<isize>,
    hits: HitRegions,
    config_mtime: Option<SystemTime>,
    shell_hook_msg: u32,
    anim_timer_on: bool,
    dirty: bool,
    suppress_deactivate_until_ms: u64,
    demo_store: Option<Arc<Mutex<MediaStore>>>,
    demo_quit_on_close: bool,
}

impl AppState {
    fn sync_overlay_visibility(&mut self) {
        let opened = self.ctrl.opened;
        let revealed = self.ctrl.revealed;
        let holding = self.ctrl.mode == SwitchMode::Hold;
        win_hook::sync_hook_flags(opened, revealed, holding);

        if !opened {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_REVEAL_ID);
                let _ = KillTimer(self.hwnd, TIMER_TICK_ID);
                self.anim_timer_on = false;
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            return;
        }

        if opened && !revealed {
            unsafe {
                let _ = SetTimer(
                    self.hwnd,
                    TIMER_REVEAL_ID,
                    controller::REVEAL_DELAY_MS as u32,
                    None,
                );
            }
            self.ensure_anim_timer();
            return;
        }

        // Revealed: position fullscreen over the focused monitor and bring to foreground.
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_REVEAL_ID);
            let r = self.ctrl.target_monitor;
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                r.x.round() as i32,
                r.y.round() as i32,
                r.width.round() as i32,
                r.height.round() as i32,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            win_platform::force_focus_overlay(self.hwnd);
        }
        self.dirty = true;
        self.ensure_anim_timer();
    }

    fn ensure_anim_timer(&mut self) {
        let should_run = self.ctrl.opened
            && ((self.ctrl.revealed && self.ctrl.is_animating())
                || (self.ctrl.mode == SwitchMode::Hold && !self.ctrl.demo_mode));
        unsafe {
            if should_run && !self.anim_timer_on {
                let _ = SetTimer(self.hwnd, TIMER_TICK_ID, 12, None);
                self.anim_timer_on = true;
            } else if !should_run && self.anim_timer_on {
                let _ = KillTimer(self.hwnd, TIMER_TICK_ID);
                self.anim_timer_on = false;
            }
        }
    }

    fn reload_config_if_changed(&mut self) {
        let (cfg, mtime) = Config::load_or_default();
        let auto = win_platform::read_windows_auto_theme();
        let next_palette = cfg.resolve_palette(auto);
        if mtime != self.config_mtime
            || next_palette.background != self.ctrl.palette.background
            || next_palette.accent != self.ctrl.palette.accent
        {
            self.ctrl.palette = next_palette;
            self.renderer.update_font_family(&cfg.font_family);
            self.ctrl.config = cfg;
            self.config_mtime = mtime;
            self.dirty = true;
        }
    }

    fn open_real_field(&mut self, mode: SwitchMode, step_dir: isize) {
        self.reload_config_if_changed();
        let now_ms = now_epoch_ms();

        if self.ctrl.opened {
            if step_dir != 0 {
                if !self.ctrl.pinned && mode == SwitchMode::Hold {
                    self.ctrl.mode = SwitchMode::Hold;
                }
                self.ctrl.step(step_dir, now_ms);
                self.request_visible_captures(now_ms);
                self.dirty = true;
                self.ensure_anim_timer();
                self.sync_overlay_visibility();
            }
            return;
        }

        let (monitors, target_monitor) = win_platform::enumerate_monitors();
        let vd = win_platform::VirtualDesktopSnapshot::read();
        let (candidates, workspaces, fallback_fg) = win_platform::enumerate_windows(
            self.hwnd,
            self.vdm.as_ref(),
            &vd,
            &monitors,
            &self.urgent_hwnds,
        );

        let opened = self.ctrl.open_field(
            mode,
            step_dir,
            &candidates,
            &workspaces,
            monitors,
            target_monitor,
            fallback_fg,
            now_ms,
        );
        if !opened {
            return;
        }

        // Sweep closed snapshots, decode icons for current field, and schedule captures near camera.
        let alive: HashSet<isize> = candidates.iter().map(|c| c.hwnd).collect();
        if let Ok(mut store) = self.capture.store.lock() {
            store.sweep_closed(&alive);
        }
        for entry in &self.ctrl.field {
            self.capture
                .ensure_icon_sync(self.wic.as_ref(), entry.hwnd, &entry.exe_path);
        }
        self.request_visible_captures(now_ms);

        if self.ctrl.revealed {
            self.begin_wallpaper_load(now_ms);
        }
        self.sync_overlay_visibility();
        if self.ctrl.revealed {
            self.draw_now(now_ms);
        }
    }

    fn begin_wallpaper_load(&mut self, now_ms: u64) {
        if let Some(path) = win_platform::current_wallpaper_path() {
            let w = self.ctrl.target_monitor.width.round().max(1.0) as u32;
            let h = self.ctrl.target_monitor.height.round().max(1.0) as u32;
            if self.capture.request_wallpaper(&path, w, h) {
                self.ctrl.notify_wallpaper_ready(now_ms as f64);
            }
        }
    }

    fn request_visible_captures(&self, now_ms: u64) {
        if self.ctrl.demo_mode {
            return;
        }
        for &entry_idx in &self.ctrl.order {
            let slot = self.ctrl.slots[entry_idx];
            if slot < 0 {
                continue;
            }
            let r = slot as f64 - self.ctrl.camera_slot.current;
            if (-1.2..=layout::VISIBLE_STEPS + 0.5).contains(&r) {
                let entry = &self.ctrl.field[entry_idx];
                if !entry.minimized && entry.on_current_desktop {
                    self.capture.request_window(entry.hwnd, now_ms);
                }
            }
        }
    }

    fn draw_now(&mut self, now_ms: u64) {
        if !self.ctrl.opened || !self.ctrl.revealed {
            return;
        }
        let store_arc = self
            .demo_store
            .as_ref()
            .unwrap_or(&self.capture.store)
            .clone();
        if let Ok(store) = store_arc.lock() {
            if let Ok(hits) = self
                .renderer
                .render_to_hwnd(self.hwnd, &self.ctrl, &store, now_ms)
            {
                self.hits = hits;
            }
        }
        self.dirty = false;
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Cancel => {
                self.sync_overlay_visibility();
                if self.demo_quit_on_close {
                    unsafe {
                        PostQuitMessage(0);
                    }
                }
            }
            Action::Commit {
                hwnd,
                on_current_desktop,
                workspace_key,
                minimized,
            } => {
                self.sync_overlay_visibility();
                if self.demo_quit_on_close {
                    unsafe {
                        PostQuitMessage(0);
                    }
                    return;
                }
                if !self.ctrl.demo_mode {
                    win_platform::focus_target_window(
                        hwnd,
                        on_current_desktop,
                        &workspace_key,
                        minimized,
                    );
                    self.urgent_hwnds.remove(&hwnd);
                }
            }
            Action::SwitchWorkspace { workspace_key } => {
                self.sync_overlay_visibility();
                if self.demo_quit_on_close {
                    unsafe {
                        PostQuitMessage(0);
                    }
                    return;
                }
                if !self.ctrl.demo_mode {
                    win_platform::switch_virtual_desktop_by_key(&workspace_key);
                }
            }
        }
    }

    fn create_desktop_on_page(&mut self, move_entry_idx: Option<usize>) {
        if self.ctrl.demo_mode {
            let desc = self.ctrl.next_demo_workspace_descriptor();
            let g_idx = self.ctrl.add_workspace_descriptor(desc);
            if let Some(entry_idx) = move_entry_idx {
                let _ = self.ctrl.move_entry_to_workspace_group(entry_idx, g_idx);
            }
            self.dirty = true;
            return;
        }

        self.suppress_deactivate_until_ms = now_epoch_ms().saturating_add(900);
        let move_hwnd = move_entry_idx.and_then(|i| self.ctrl.field.get(i).map(|e| e.hwnd));
        if let Some(info) =
            win_platform::create_virtual_desktop(self.hwnd, move_hwnd, self.vdm.as_ref())
        {
            let key = format!("vd:{}", win_platform::format_guid_braced(info.guid));
            let monitor = self
                .ctrl
                .monitors
                .values()
                .find(|m| m.focused)
                .or_else(|| self.ctrl.monitors.values().next())
                .map(|m| m.name.clone())
                .unwrap_or_default();
            let desc = field::WorkspaceDescriptor {
                key,
                id: Some(info.index_one_based as i64),
                name: info.name,
                monitor,
                on_screen: info.is_current,
            };
            let g_idx = self.ctrl.add_workspace_descriptor(desc);
            if let Some(entry_idx) = move_entry_idx {
                let _ = self.ctrl.move_entry_to_workspace_group(entry_idx, g_idx);
            }
            self.dirty = true;
        }
    }

    fn move_entry_to_group(&mut self, entry_idx: usize, group_idx: usize) {
        let Some(target_key) = self.ctrl.groups.get(group_idx).map(|g| g.key.clone()) else {
            return;
        };
        let Some(entry) = self.ctrl.field.get(entry_idx) else {
            return;
        };
        if entry.workspace_key == target_key {
            return;
        }
        let moved_ok = if self.ctrl.demo_mode {
            true
        } else {
            win_platform::move_window_to_desktop_key(entry.hwnd, &target_key, self.vdm.as_ref())
        };
        if moved_ok {
            let _ = self
                .ctrl
                .move_entry_to_workspace_group(entry_idx, group_idx);
            self.dirty = true;
        }
    }

    fn handle_vk(&mut self, vk: u16, shift: bool, ctrl_down: bool) -> bool {
        if !self.ctrl.opened {
            return false;
        }
        let now_ms = now_epoch_ms();
        self.ctrl.note_input(now_ms);
        let handled = match vk {
            x if x == VK_TAB.0 => self.ctrl.step(if shift { -1 } else { 1 }, now_ms),
            x if x == VK_DOWN.0 => self.ctrl.move_by(1, now_ms),
            x if x == VK_UP.0 => self.ctrl.move_by(-1, now_ms),
            x if x == VK_RIGHT.0 => self.ctrl.workspace_step(1, now_ms),
            x if x == VK_LEFT.0 => self.ctrl.workspace_step(-1, now_ms),
            x if x == VK_NEXT.0 => self.ctrl.move_by(5, now_ms),
            x if x == VK_PRIOR.0 => self.ctrl.move_by(-5, now_ms),
            x if x == VK_HOME.0 => self
                .ctrl
                .order
                .first()
                .copied()
                .map(|idx| self.ctrl.jump_to(idx, now_ms))
                .unwrap_or(false),
            x if x == VK_END.0 => self
                .ctrl
                .order
                .last()
                .copied()
                .map(|idx| self.ctrl.jump_to(idx, now_ms))
                .unwrap_or(false),
            x if x == VK_RETURN.0 => {
                let act = self.ctrl.commit();
                self.handle_action(act);
                return true;
            }
            x if x == VK_ESCAPE.0 => {
                if !self.ctrl.filter_text.is_empty() {
                    self.ctrl.set_filter(String::new(), now_ms)
                } else {
                    let act = self.ctrl.cancel();
                    self.handle_action(act);
                    return true;
                }
            }
            x if x == VK_BACK.0 => {
                let next = if ctrl_down {
                    String::new()
                } else {
                    let mut s = self.ctrl.filter_text.clone();
                    s.pop();
                    s
                };
                self.ctrl.set_filter(next, now_ms)
            }
            x if x == VK_SPACE.0 => {
                if !self.ctrl.filter_text.is_empty() {
                    let next = format!("{} ", self.ctrl.filter_text);
                    self.ctrl.set_filter(next, now_ms)
                } else if self.ctrl.mode == SwitchMode::Hold {
                    let ok = self.ctrl.pin();
                    self.sync_overlay_visibility();
                    ok
                } else {
                    false
                }
            }
            _ => {
                if ctrl_down {
                    return false;
                }
                if let Some(ch) = vk_to_char(vk, shift) {
                    if self.ctrl.filter_text.is_empty() && ('1'..='9').contains(&ch) {
                        let num = ch.to_digit(10).unwrap() as i64;
                        self.ctrl.workspace_jump(num, now_ms)
                    } else {
                        let next = format!("{}{}", self.ctrl.filter_text, ch);
                        self.ctrl.set_filter(next, now_ms)
                    }
                } else {
                    false
                }
            }
        };

        if handled && self.ctrl.opened {
            self.request_visible_captures(now_ms);
            self.dirty = true;
            self.ensure_anim_timer();
            if self.ctrl.revealed {
                self.draw_now(now_ms);
            }
        }
        handled
    }

    fn pick_sounding_line(&mut self, x: f64, y: f64, now_ms: u64) {
        if self.hits.gauge_marks.is_empty() {
            return;
        }
        let col = ((x - self.hits.gauge_marks_x - self.hits.gauge_mark_size / 2.0)
            / self.hits.gauge_mark_step.max(1.0))
        .round()
        .max(0.0) as usize;
        if let Some(mark_idx) = layout::nearest_mark(&self.hits.gauge_marks, y, col) {
            if let Some(&entry_idx) = self.hits.gauge_shown_indices.get(mark_idx) {
                self.ctrl.jump_to(entry_idx, now_ms);
                self.request_visible_captures(now_ms);
                self.dirty = true;
                self.ensure_anim_timer();
                self.draw_now(now_ms);
            }
        }
    }
}

fn vk_to_char(vk: u16, shift: bool) -> Option<char> {
    if (0x41..=0x5A).contains(&vk) {
        let base = (vk as u8) as char;
        return Some(if shift {
            base
        } else {
            base.to_ascii_lowercase()
        });
    }
    if (0x30..=0x39).contains(&vk) && !shift {
        return Some((vk as u8) as char);
    }
    let mut state = [0u8; 256];
    if shift {
        state[VK_SHIFT.0 as usize] = 0x80;
    }
    let mut buf = [0u16; 4];
    let n = unsafe { ToUnicode(vk as u32, 0, Some(&state), &mut buf, 0) };
    if n == 1 {
        let ch = char::from_u32(buf[0] as u32)?;
        if !ch.is_control() {
            return Some(ch);
        }
    }
    None
}

fn point_in(r: Rect, x: f64, y: f64) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
        let cs = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            cs.lpCreateParams as isize,
        );
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
    ) as *mut AppState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    if msg == state.shell_hook_msg && state.shell_hook_msg != 0 {
        if wparam.0 == HSHELL_FLASH {
            state.urgent_hwnds.insert(lparam.0);
        }
        return LRESULT(0);
    }

    match msg {
        WM_FATHOM_CHORD_STEP => {
            let delta = wparam.0 as isize;
            state.open_real_field(SwitchMode::Hold, delta);
            LRESULT(0)
        }
        WM_FATHOM_OPEN_BROWSE => {
            if state.ctrl.opened {
                let act = state.ctrl.cancel();
                state.handle_action(act);
            } else {
                state.open_real_field(SwitchMode::Browse, 0);
            }
            LRESULT(0)
        }
        WM_FATHOM_ALT_RELEASE => {
            let act = state.ctrl.alt_released();
            state.handle_action(act);
            LRESULT(0)
        }
        WM_FATHOM_PRE_REVEAL_KEY => {
            let vk = wparam.0 as u16;
            let shift = lparam.0 != 0;
            state.handle_vk(vk, shift, false);
            LRESULT(0)
        }
        WM_FATHOM_FOCUS_CHANGED => {
            let target = wparam.0 as isize;
            if target != state.hwnd.0 as isize && !state.ctrl.opened {
                state.urgent_hwnds.remove(&target);
                state.ctrl.recency.record_focus(target, now_epoch_ms());
            }
            LRESULT(0)
        }
        WM_FATHOM_WINDOW_OPENED => {
            let target = wparam.0 as isize;
            if target != state.hwnd.0 as isize {
                state.ctrl.recency.record_open(target, now_epoch_ms());
            }
            LRESULT(0)
        }
        WM_FATHOM_WINDOW_CLOSED => {
            let target = wparam.0 as isize;
            state.ctrl.recency.record_close(target);
            state.urgent_hwnds.remove(&target);
            if let Ok(mut store) = state.capture.store.lock() {
                store.drop_snapshot(target);
            }
            if state.ctrl.opened {
                if let Some(act) = state.ctrl.mark_closed(target, now_epoch_ms()) {
                    state.handle_action(act);
                } else {
                    state.dirty = true;
                    state.draw_now(now_epoch_ms());
                }
            }
            LRESULT(0)
        }
        WM_FATHOM_CAPTURE_READY => {
            if state.ctrl.opened && state.ctrl.revealed {
                state.dirty = true;
                state.draw_now(now_epoch_ms());
            }
            LRESULT(0)
        }
        WM_FATHOM_WALLPAPER_READY => {
            let now_ms = now_epoch_ms();
            state.ctrl.notify_wallpaper_ready(now_ms as f64);
            state.ensure_anim_timer();
            if state.ctrl.opened && state.ctrl.revealed {
                state.draw_now(now_ms);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let now_ms = now_epoch_ms();
            if wparam.0 == TIMER_REVEAL_ID {
                let _ = KillTimer(hwnd, TIMER_REVEAL_ID);
                if state.ctrl.opened && !state.ctrl.revealed {
                    state.ctrl.reveal_now(now_ms as f64);
                    state.begin_wallpaper_load(now_ms);
                    state.sync_overlay_visibility();
                    state.draw_now(now_ms);
                }
            } else if wparam.0 == TIMER_TICK_ID {
                if state.ctrl.opened
                    && state.ctrl.revealed
                    && state.ctrl.mode == SwitchMode::Hold
                    && !state.ctrl.demo_mode
                    && !win_hook::is_alt_down()
                {
                    let act = state.ctrl.alt_released();
                    state.handle_action(act);
                    return LRESULT(0);
                }
                if let Some(act) = state.ctrl.check_timers(now_ms) {
                    state.handle_action(act);
                    return LRESULT(0);
                }
                let ticked = state.ctrl.tick_animations(now_ms);
                if ticked || state.dirty {
                    state.draw_now(now_ms);
                }
                state.ensure_anim_timer();
            }
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            // Ignore raw Alt modifier press itself so holding Alt never beeps or blurs.
            if vk == VK_MENU.0 {
                return LRESULT(0);
            }
            let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let ctrl_down = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            if state.handle_vk(vk, shift, ctrl_down) {
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_SYSCHAR | WM_CHAR => LRESULT(0),
        WM_SYSCOMMAND => {
            if (wparam.0 & 0xFFF0) == SC_KEYMENU as usize {
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ACTIVATE => {
            let active_state = (wparam.0 & 0xFFFF) as u16;
            let now_ms = now_epoch_ms();
            if active_state == 0
                && state.ctrl.opened
                && state.ctrl.revealed
                && now_ms.saturating_sub(state.ctrl.revealed_at_ms) > 150
                && now_ms >= state.suppress_deactivate_until_ms
            {
                let act = state.ctrl.cancel();
                state.handle_action(act);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if !state.ctrl.opened || !state.ctrl.revealed {
                return LRESULT(0);
            }
            let x = (lparam.0 & 0xFFFF) as i16 as f64;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f64;
            let now_ms = now_epoch_ms();
            state.ctrl.note_input(now_ms);

            if let Some(gr) = state.hits.gauge_rect {
                if point_in(gr, x, y) {
                    state.ctrl.dragging_gauge = true;
                    SetCapture(hwnd);
                    state.pick_sounding_line(x, y, now_ms);
                    return LRESULT(0);
                }
            }
            if let Some(nr) = state.hits.new_desktop_rect {
                if point_in(nr, x, y) {
                    state.create_desktop_on_page(None);
                    state.draw_now(now_ms);
                    return LRESULT(0);
                }
            }
            if let Some(cr) = state.hits.caption_rect {
                if point_in(cr, x, y) && state.ctrl.selected_entry().is_some() {
                    let act = state.ctrl.commit();
                    state.handle_action(act);
                    return LRESULT(0);
                }
            }
            if let Some(fr) = state.hits.filter_rect {
                if point_in(fr, x, y) {
                    return LRESULT(0);
                }
            }
            if let Some(hr) = state.hits.hints_rect {
                if point_in(hr, x, y) {
                    return LRESULT(0);
                }
            }
            for &(entry_idx, r) in &state.hits.plane_rects {
                if point_in(r, x, y) {
                    state.ctrl.drag_pending = Some((entry_idx, x, y));
                    SetCapture(hwnd);
                    return LRESULT(0);
                }
            }
            for &(entry_idx, r) in &state.hits.map_tiles {
                if point_in(r, x, y) {
                    state.ctrl.drag_pending = Some((entry_idx, x, y));
                    SetCapture(hwnd);
                    return LRESULT(0);
                }
            }
            for &(g_idx, r) in &state.hits.workspace_cards {
                if point_in(r, x, y) {
                    if let Some(front) =
                        field::group_front(&state.ctrl.groups[g_idx], &state.ctrl.order)
                    {
                        state.ctrl.selected_index = Some(front);
                        let act = state.ctrl.commit();
                        state.handle_action(act);
                    } else {
                        let act = state.ctrl.switch_to_workspace(g_idx);
                        state.handle_action(act);
                    }
                    return LRESULT(0);
                }
            }

            // Click on empty space closes without focusing
            let act = state.ctrl.cancel();
            state.handle_action(act);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if state.ctrl.dragging_gauge {
                state.ctrl.dragging_gauge = false;
                let _ = ReleaseCapture();
                return LRESULT(0);
            }
            let now_ms = now_epoch_ms();
            if let Some(entry_idx) = state.ctrl.dragging_entry {
                let drop_new = state.ctrl.drop_new_desktop;
                let drop_grp = state.ctrl.drop_target_group;
                state.ctrl.clear_drag();
                let _ = ReleaseCapture();
                if drop_new {
                    state.create_desktop_on_page(Some(entry_idx));
                } else if let Some(g_idx) = drop_grp {
                    state.move_entry_to_group(entry_idx, g_idx);
                }
                state.request_visible_captures(now_ms);
                state.draw_now(now_ms);
                return LRESULT(0);
            }
            if let Some((entry_idx, _, _)) = state.ctrl.drag_pending.take() {
                let _ = ReleaseCapture();
                state.ctrl.selected_index = Some(entry_idx);
                let act = state.ctrl.commit();
                state.handle_action(act);
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !state.ctrl.opened || !state.ctrl.revealed {
                return LRESULT(0);
            }
            let x = (lparam.0 & 0xFFFF) as i16 as f64;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f64;
            let now_ms = now_epoch_ms();

            if state.ctrl.dragging_gauge {
                state.pick_sounding_line(x, y, now_ms);
                return LRESULT(0);
            }

            if let Some((entry_idx, px, py)) = state.ctrl.drag_pending {
                if (x - px).hypot(y - py) >= 6.0 {
                    state.ctrl.drag_pending = None;
                    state.ctrl.dragging_entry = Some(entry_idx);
                    state.ctrl.drag_pos = (x, y);
                }
            }

            if state.ctrl.dragging_entry.is_some() {
                state.ctrl.drag_pos = (x, y);
                state.ctrl.drop_new_desktop = state
                    .hits
                    .new_desktop_rect
                    .map(|r| point_in(r, x, y))
                    .unwrap_or(false);
                state.ctrl.drop_target_group = if state.ctrl.drop_new_desktop {
                    None
                } else {
                    state
                        .hits
                        .workspace_cards
                        .iter()
                        .find(|&&(_, r)| point_in(r, x, y))
                        .map(|&(g_idx, _)| g_idx)
                };
                state.ctrl.note_input(now_ms);
                state.draw_now(now_ms);
                return LRESULT(0);
            }

            let moved = state.ctrl.pointer_moved(x, y, now_ms);
            let mut next_hover_plane = None;
            for &(entry_idx, r) in &state.hits.plane_rects {
                if point_in(r, x, y) {
                    next_hover_plane = Some(entry_idx);
                    break;
                }
            }
            let mut next_hover_tile = None;
            for &(entry_idx, r) in &state.hits.map_tiles {
                if point_in(r, x, y) {
                    next_hover_tile = Some(entry_idx);
                    break;
                }
            }
            let next_hover_new = state
                .hits
                .new_desktop_rect
                .map(|r| point_in(r, x, y))
                .unwrap_or(false);

            let mut need_draw = false;
            if next_hover_plane != state.ctrl.hovered_plane {
                state.ctrl.hovered_plane = next_hover_plane;
                need_draw = true;
            }
            if next_hover_tile != state.ctrl.hovered_tile {
                state.ctrl.hovered_tile = next_hover_tile;
                need_draw = true;
            }
            if next_hover_new != state.ctrl.hovered_new_desktop {
                state.ctrl.hovered_new_desktop = next_hover_new;
                need_draw = true;
            }
            if moved {
                if let Some(tile_entry) = next_hover_tile {
                    if state.ctrl.slots.get(tile_entry).copied().unwrap_or(-1) >= 0
                        && state.ctrl.selected_index != Some(tile_entry)
                    {
                        state.ctrl.jump_to(tile_entry, now_ms);
                        state.request_visible_captures(now_ms);
                        state.ensure_anim_timer();
                        need_draw = true;
                    }
                }
            }
            if need_draw {
                state.draw_now(now_ms);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if state.ctrl.opened && state.ctrl.revealed {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f64;
                let now_ms = now_epoch_ms();
                state.ctrl.wheel(delta, 0.0, 0.0, 0.0, now_ms);
                state.request_visible_captures(now_ms);
                state.dirty = true;
                state.ensure_anim_timer();
                state.draw_now(now_ms);
            }
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            if state.ctrl.opened && state.ctrl.revealed {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f64;
                let now_ms = now_epoch_ms();
                state.ctrl.wheel(0.0, 0.0, delta, 0.0, now_ms);
                state.request_visible_captures(now_ms);
                state.dirty = true;
                state.ensure_anim_timer();
                state.draw_now(now_ms);
            }
            LRESULT(0)
        }
        tray::WM_TRAY_ICON => {
            let ev = (lparam.0 & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP {
                if let Some(ref tray) = state.tray {
                    tray.show_menu(win_platform::is_process_elevated());
                }
            } else if ev == WM_LBUTTONUP {
                state.open_real_field(SwitchMode::Browse, 0);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd = wparam.0 & 0xFFFF;
            match cmd {
                tray::CMD_BROWSE => state.open_real_field(SwitchMode::Browse, 0),
                tray::CMD_OPEN_CONFIG => tray::open_config_in_editor(),
                tray::CMD_AUTOSTART => tray::toggle_autostart(),
                tray::CMD_ELEVATE => {
                    if win_platform::relaunch_elevated(&["--replace".to_string()]) {
                        PostQuitMessage(0);
                    }
                }
                tray::CMD_EXIT => PostQuitMessage(0),
                _ => {}
            }
            LRESULT(0)
        }
        WM_FATHOM_QUIT => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_CLOSE => {
            if state.ctrl.opened {
                let act = state.ctrl.cancel();
                state.handle_action(act);
            } else {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    win_platform::enable_dpi_awareness();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!(
            "Winscroller Alt+Tab v{}\n\n\
             USAGE:\n  winscroller-alt-tab.exe [OPTIONS]\n\n\
             OPTIONS:\n  \
               --browse                Open immediately in Browse mode (or wake running instance)\n  \
               --no-elevate            Run without requesting Administrator UAC elevation\n  \
               --replace               Terminate any existing instance and start fresh\n  \
               --quit                  Gracefully exit any currently running instance\n  \
               --init-config           Create default config.toml at %APPDATA%\\winscroller-alt-tab\\\n  \
               --validate              Validate config.toml syntax and values\n  \
               --demo                  Launch interactive 12-window / 8-workspace demo\n  \
               --render-demo <path>    Render offscreen 1920x1080 demo PNG and exit\n  \
               --render-gallery <dir>  Render feature screenshots and animation frames\n  \
               -V, --version           Print version information\n  \
               -h, --help              Print this help message",
            env!("CARGO_PKG_VERSION")
        );
        unsafe {
            CoUninitialize();
        }
        return;
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!("winscroller-alt-tab {}", env!("CARGO_PKG_VERSION"));
        unsafe {
            CoUninitialize();
        }
        return;
    }

    if args.iter().any(|a| a == "--init-config") {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        match Config::ensure_default_file() {
            Some(path) => println!("Config ready at: {}", path.display()),
            None => {
                eprintln!("Error: %APPDATA% environment variable is not set.");
                unsafe {
                    CoUninitialize();
                }
                std::process::exit(1);
            }
        }
        unsafe {
            CoUninitialize();
        }
        return;
    }

    if args.iter().any(|a| a == "--validate") {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        match Config::config_path() {
            Some(path) if path.exists() => match std::fs::read_to_string(&path) {
                Ok(contents) => match Config::validate_toml(&contents) {
                    Ok(_) => println!("Configuration valid: {}", path.display()),
                    Err(errors) => {
                        eprintln!("Configuration errors in {}:", path.display());
                        for err in errors {
                            eprintln!("  - {err}");
                        }
                        unsafe {
                            CoUninitialize();
                        }
                        std::process::exit(1);
                    }
                },
                Err(err) => {
                    eprintln!("Failed to read {}: {err}", path.display());
                    unsafe {
                        CoUninitialize();
                    }
                    std::process::exit(1);
                }
            },
            Some(path) => {
                println!(
                    "No config file at {} (defaults are valid; run --init-config to create one).",
                    path.display()
                );
            }
            None => {
                eprintln!("Error: %APPDATA% environment variable is not set.");
                unsafe {
                    CoUninitialize();
                }
                std::process::exit(1);
            }
        }
        unsafe {
            CoUninitialize();
        }
        return;
    }

    if args.iter().any(|a| a == "--quit") {
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            if let Ok(existing) = FindWindowW(WINDOW_CLASS, None) {
                if !existing.0.is_null() {
                    let _ = PostMessageW(existing, WM_FATHOM_QUIT, WPARAM(0), LPARAM(0));
                    println!("Sent quit signal to running Winscroller Alt+Tab instance.");
                } else {
                    println!("No running Winscroller Alt+Tab instance found.");
                }
            } else {
                println!("No running Winscroller Alt+Tab instance found.");
            }
            CoUninitialize();
        }
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--render-demo") {
        let out_path = args
            .get(pos + 1)
            .map(|s| s.as_str())
            .unwrap_or("winscroller_demo.png");
        let (cfg, _) = Config::load_or_default();
        let pal = cfg.resolve_palette(win_platform::read_windows_auto_theme());
        let mut ctrl = Controller::new(cfg.clone(), pal);
        let mut store = MediaStore::default();
        let now_ms = now_epoch_ms();
        demo::setup_demo(&mut ctrl, &mut store, 1920.0, 1080.0, now_ms);
        {
            let mut renderer = Renderer::new(&cfg.font_family).expect("init D2D renderer");
            let wic = win_capture::create_wic_factory().expect("WIC factory");
            renderer
                .render_to_png(&wic, out_path, &ctrl, &store, 1920, 1080, now_ms)
                .expect("render demo png");
        }
        unsafe {
            CoUninitialize();
        }
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--render-gallery") {
        let out_dir = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("docs");
        let (cfg, _) = Config::load_or_default();
        let now_ms = now_epoch_ms();
        {
            let mut renderer = Renderer::new(&cfg.font_family).expect("init D2D renderer");
            let wic = win_capture::create_wic_factory().expect("WIC factory");
            demo::render_gallery(out_dir, &wic, &mut renderer, &cfg, now_ms)
                .expect("render demo gallery");
        }
        unsafe {
            CoUninitialize();
        }
        return;
    }

    let browse_now = args.iter().any(|a| a == "--browse");
    let demo_now = args.iter().any(|a| a == "--demo");
    let no_elevate = args.iter().any(|a| a == "--no-elevate");
    let replace_existing = args.iter().any(|a| a == "--replace");

    if !demo_now && !no_elevate && !win_platform::is_process_elevated() {
        let mut forward = args.clone();
        if !forward.iter().any(|a| a == "--replace") {
            forward.push("--replace".to_string());
        }
        if win_platform::relaunch_elevated(&forward) {
            unsafe {
                CoUninitialize();
            }
            return;
        }
    }

    if !demo_now {
        if replace_existing {
            unsafe {
                if let Ok(existing) = FindWindowW(WINDOW_CLASS, None) {
                    if !existing.0.is_null() {
                        let _ = PostMessageW(existing, WM_FATHOM_QUIT, WPARAM(0), LPARAM(0));
                        for _ in 0..20 {
                            std::thread::sleep(std::time::Duration::from_millis(25));
                            if FindWindowW(WINDOW_CLASS, None)
                                .map(|h| h.0.is_null())
                                .unwrap_or(true)
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }
        let mutex_name = w!("Local\\WinscrollerAltTabMutex");
        unsafe {
            let _ = CreateMutexW(None, false, mutex_name);
            if GetLastError().0 == ERROR_ALREADY_EXISTS {
                if let Ok(existing) = FindWindowW(WINDOW_CLASS, None) {
                    if !existing.0.is_null() {
                        let _ = PostMessageW(existing, WM_FATHOM_OPEN_BROWSE, WPARAM(0), LPARAM(0));
                    }
                }
                CoUninitialize();
                return;
            }
        }
    }

    let (cfg, mtime) = Config::load_or_default();
    let pal = cfg.resolve_palette(win_platform::read_windows_auto_theme());
    let ctrl = Controller::new(cfg.clone(), pal);
    let renderer = Renderer::new(&cfg.font_family).expect("failed to initialize Direct2D/D3D11");
    let wic = win_capture::create_wic_factory();
    let vdm = win_platform::create_virtual_desktop_manager();

    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);

        // Placeholder capture service before HWND is allocated; replaced right after CreateWindowExW.
        let dummy_capture = CaptureService::start(0);
        let boxed = Box::new(AppState {
            hwnd: HWND::default(),
            ctrl,
            renderer,
            capture: dummy_capture,
            wic,
            vdm,
            tray: None,
            urgent_hwnds: HashSet::new(),
            hits: HitRegions::default(),
            config_mtime: mtime,
            shell_hook_msg: RegisterWindowMessageW(w!("SHELLHOOK")),
            anim_timer_on: false,
            dirty: false,
            suppress_deactivate_until_ms: 0,
            demo_store: None,
            demo_quit_on_close: demo_now,
        });
        let raw_ptr = Box::into_raw(boxed);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            WINDOW_CLASS,
            w!("Winscroller Alt+Tab"),
            WS_POPUP,
            0,
            0,
            1920,
            1080,
            None,
            None,
            hinstance,
            Some(raw_ptr as *const _),
        )
        .expect("CreateWindowExW failed");

        let state = &mut *raw_ptr;
        state.hwnd = hwnd;
        state.capture = CaptureService::start(hwnd.0 as isize);

        if demo_now {
            let (_, target_rect) = win_platform::enumerate_monitors();
            let mut store = MediaStore::default();
            let now_ms = now_epoch_ms();
            demo::setup_demo(
                &mut state.ctrl,
                &mut store,
                target_rect.width,
                target_rect.height,
                now_ms,
            );
            state.ctrl.target_monitor = target_rect;
            state.demo_store = Some(Arc::new(Mutex::new(store)));
            state.sync_overlay_visibility();
            state.draw_now(now_ms);
        } else {
            state.tray = Some(TrayIcon::create(hwnd));
            let _ = RegisterShellHookWindow(hwnd);
            // Seed recency from existing windows immediately at startup.
            let (monitors, _) = win_platform::enumerate_monitors();
            let vd = win_platform::VirtualDesktopSnapshot::read();
            let (cands, _, fg) = win_platform::enumerate_windows(
                hwnd,
                state.vdm.as_ref(),
                &vd,
                &monitors,
                &state.urgent_hwnds,
            );
            let now_ms = now_epoch_ms();
            state.ctrl.recency.seed_from_candidates(&cands, now_ms);
            if let Some(f) = fg {
                state.ctrl.recency.active = Some(f);
            }
        }

        let _hooks = if !demo_now {
            Some(HooksGuard::install(hwnd))
        } else {
            None
        };

        if browse_now && !demo_now {
            let _ = PostMessageW(hwnd, WM_FATHOM_OPEN_BROWSE, WPARAM(0), LPARAM(0));
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = _hooks;
        drop(_hooks);
        drop(Box::from_raw(raw_ptr));
        CoUninitialize();
    }
}
