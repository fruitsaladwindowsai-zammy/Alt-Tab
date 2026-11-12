//! Dedicated-thread `WH_KEYBOARD_LL` Alt+Tab interceptor and `SetWinEventHook` focus tracker.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_OEM_3,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

pub const WM_FATHOM_CHORD_STEP: u32 = 0x8000 + 1;
pub const WM_FATHOM_ALT_RELEASE: u32 = 0x8000 + 2;
pub const WM_FATHOM_OPEN_BROWSE: u32 = 0x8000 + 3;
pub const WM_FATHOM_PRE_REVEAL_KEY: u32 = 0x8000 + 4;
pub const WM_FATHOM_FOCUS_CHANGED: u32 = 0x8000 + 5;
pub const WM_FATHOM_WINDOW_OPENED: u32 = 0x8000 + 6;
pub const WM_FATHOM_WINDOW_CLOSED: u32 = 0x8000 + 7;

static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);
static FATHOM_OPEN: AtomicBool = AtomicBool::new(false);
static FATHOM_REVEALED: AtomicBool = AtomicBool::new(false);
static FATHOM_HOLDING: AtomicBool = AtomicBool::new(false);
static ALT_TRACKED_DOWN: AtomicBool = AtomicBool::new(false);
static TAB_SWALLOWED: AtomicBool = AtomicBool::new(false);
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

pub fn sync_hook_flags(opened: bool, revealed: bool, holding: bool) {
    FATHOM_OPEN.store(opened, Ordering::SeqCst);
    FATHOM_REVEALED.store(revealed, Ordering::SeqCst);
    FATHOM_HOLDING.store(holding, Ordering::SeqCst);
}

pub fn is_alt_down() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LMENU.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RMENU.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0
    }
}

fn is_shift_down() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LSHIFT.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RSHIFT.0 as i32) as u16 & 0x8000) != 0
    }
}

fn is_ctrl_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 }
}

fn is_win_down() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0
    }
}

pub struct HooksGuard {
    win_hooks: Vec<HWINEVENTHOOK>,
    kb_thread: Option<thread::JoinHandle<()>>,
}

impl HooksGuard {
    pub fn install(target_hwnd: HWND) -> Self {
        TARGET_HWND.store(target_hwnd.0 as isize, Ordering::SeqCst);

        // Run WH_KEYBOARD_LL on its own dedicated thread so heavy Direct2D rendering or
        // window enumeration never stalls the hook past Windows' LowLevelHooksTimeout.
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();
        let kb_thread = thread::Builder::new()
            .name("winscroller-kb-hook".to_string())
            .spawn(move || unsafe {
                let tid = GetCurrentThreadId();
                HOOK_THREAD_ID.store(tid, Ordering::SeqCst);
                let hmod = GetModuleHandleW(None).ok();
                let kb_hook = match hmod {
                    Some(m) => SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), HINSTANCE::from(m), 0).ok(),
                    None => SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0).ok(),
                };
                let _ = ready_tx.send(kb_hook.is_some());

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                if let Some(h) = kb_hook {
                    let _ = UnhookWindowsHookEx(h);
                }
                HOOK_THREAD_ID.store(0, Ordering::SeqCst);
            })
            .ok();

        let _ = ready_rx.recv();

        let mut win_hooks = Vec::new();
        for (min_ev, max_ev) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY),
        ] {
            let h = unsafe {
                SetWinEventHook(
                    min_ev,
                    max_ev,
                    None,
                    Some(win_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if !h.is_invalid() {
                win_hooks.push(h);
            }
        }

        Self { win_hooks, kb_thread }
    }
}

impl Drop for HooksGuard {
    fn drop(&mut self) {
        unsafe {
            let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
            if tid != 0 {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            for h in self.win_hooks.drain(..) {
                let _ = UnhookWinEvent(h);
            }
        }
        if let Some(handle) = self.kb_thread.take() {
            let _ = handle.join();
        }
    }
}

unsafe extern "system" fn ll_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let vk = kb.vkCode as u16;
    let target = HWND(TARGET_HWND.load(Ordering::Relaxed) as *mut _);

    if vk == VK_LMENU.0 || vk == VK_RMENU.0 || vk == VK_MENU.0 {
        if is_down {
            ALT_TRACKED_DOWN.store(true, Ordering::Relaxed);
        } else if is_up {
            ALT_TRACKED_DOWN.store(false, Ordering::Relaxed);
            if FATHOM_OPEN.load(Ordering::Relaxed) && FATHOM_HOLDING.load(Ordering::Relaxed) {
                let _ = PostMessageW(target, WM_FATHOM_ALT_RELEASE, WPARAM(0), LPARAM(0));
            }
        }
        // Always pass modifier transitions to the OS so Alt never gets stuck down.
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if is_up && vk == VK_TAB.0 && TAB_SWALLOWED.swap(false, Ordering::Relaxed) {
        return LRESULT(1);
    }

    if is_down {
        let alt = ALT_TRACKED_DOWN.load(Ordering::Relaxed)
            || is_alt_down()
            || (kb.flags.0 & 0x20) != 0; // LLKHF_ALTDOWN = 0x20
        let ctrl = is_ctrl_down();
        let win = is_win_down();

        // Alt+Tab / Alt+Shift+Tab -> Hold chord step (swallow Tab so Windows switcher never opens).
        if vk == VK_TAB.0 && alt && !ctrl && !win {
            TAB_SWALLOWED.store(true, Ordering::Relaxed);
            let delta: isize = if is_shift_down() { -1 } else { 1 };
            let _ = PostMessageW(target, WM_FATHOM_CHORD_STEP, WPARAM(delta as usize), LPARAM(0));
            return LRESULT(1);
        }

        // Ctrl+Alt+Space or Alt+` -> Browse mode toggle.
        if ((vk == VK_SPACE.0 && alt && ctrl && !win) || (vk == VK_OEM_3.0 && alt && !ctrl && !win))
            && !FATHOM_OPEN.load(Ordering::Relaxed)
        {
            let _ = PostMessageW(target, WM_FATHOM_OPEN_BROWSE, WPARAM(0), LPARAM(0));
            return LRESULT(1);
        }

        // During the 90 ms pre-reveal hold window, intercept navigation keys before the
        // foreground app sees them and forward to Winscroller.
        if FATHOM_OPEN.load(Ordering::Relaxed) && !FATHOM_REVEALED.load(Ordering::Relaxed) {
            if vk == VK_ESCAPE.0 || vk == VK_SPACE.0 || vk == VK_TAB.0 {
                let shift_bit = if is_shift_down() { 1usize } else { 0 };
                let _ = PostMessageW(target, WM_FATHOM_PRE_REVEAL_KEY, WPARAM(vk as usize), LPARAM(shift_bit as isize));
                return LRESULT(1);
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let target = TARGET_HWND.load(Ordering::Relaxed);
    let raw = hwnd.0 as isize;
    if raw == target {
        return;
    }
    let target_hwnd = HWND(target as *mut _);
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            let _ = PostMessageW(target_hwnd, WM_FATHOM_FOCUS_CHANGED, WPARAM(raw as usize), LPARAM(0));
        }
        EVENT_OBJECT_SHOW => {
            let _ = PostMessageW(target_hwnd, WM_FATHOM_WINDOW_OPENED, WPARAM(raw as usize), LPARAM(0));
        }
        EVENT_OBJECT_DESTROY => {
            let _ = PostMessageW(target_hwnd, WM_FATHOM_WINDOW_CLOSED, WPARAM(raw as usize), LPARAM(0));
        }
        _ => {}
    }
}
