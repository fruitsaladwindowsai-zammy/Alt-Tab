//! Asynchronous window capture (`PrintWindow`), in-memory snapshot cache, HICON & wallpaper decoding.

use crate::controller::now_epoch_ms;
use crate::win_platform;
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
    GetDC, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HALFTONE, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetWindowRect, IsIconic, PostMessageW, HICON};

pub const WM_FATHOM_CAPTURE_READY: u32 = 0x8000 + 20;
pub const WM_FATHOM_WALLPAPER_READY: u32 = 0x8000 + 21;

pub const SNAPSHOT_LIMIT: usize = 24;
pub const SNAPSHOT_WIDTH: u32 = 640;
pub const SNAPSHOT_FRESH_MS: u64 = 30_000;

#[derive(Clone)]
pub struct BgraImage {
    pub width: u32,
    pub height: u32,
    pub bgra_premul: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub struct SnapshotEntry {
    pub image: BgraImage,
    pub taken_at_ms: u64,
    pub generation: u64,
}

#[derive(Default)]
pub struct MediaStore {
    pub snapshots: HashMap<isize, SnapshotEntry>,
    pub icons: HashMap<isize, Option<BgraImage>>,
    pub wallpaper: Option<(String, BgraImage)>,
    next_gen: u64,
}

impl MediaStore {
    pub fn wants_snapshot(&self, hwnd: isize, now_ms: u64) -> bool {
        match self.snapshots.get(&hwnd) {
            None => true,
            Some(entry) => now_ms.saturating_sub(entry.taken_at_ms) > SNAPSHOT_FRESH_MS,
        }
    }

    pub fn keep_snapshot(&mut self, hwnd: isize, image: BgraImage, now_ms: u64) {
        self.next_gen = self.next_gen.wrapping_add(1);
        self.snapshots.insert(
            hwnd,
            SnapshotEntry {
                image,
                taken_at_ms: now_ms,
                generation: self.next_gen,
            },
        );
        if self.snapshots.len() > SNAPSHOT_LIMIT {
            let mut entries: Vec<(isize, u64)> = self
                .snapshots
                .iter()
                .map(|(&k, v)| (k, v.taken_at_ms))
                .collect();
            entries.sort_by_key(|e| e.1);
            let remove_count = self.snapshots.len() - SNAPSHOT_LIMIT;
            for (k, _) in entries.into_iter().take(remove_count) {
                self.snapshots.remove(&k);
            }
        }
    }

    pub fn drop_snapshot(&mut self, hwnd: isize) {
        self.snapshots.remove(&hwnd);
        self.icons.remove(&hwnd);
    }

    pub fn sweep_closed(&mut self, alive: &HashSet<isize>) {
        self.snapshots.retain(|k, _| alive.contains(k));
        self.icons.retain(|k, _| alive.contains(k));
    }
}

pub enum CaptureJob {
    Window { hwnd: isize },
    Wallpaper { path: String, max_w: u32, max_h: u32 },
}

pub struct CaptureService {
    tx: mpsc::Sender<CaptureJob>,
    in_flight: Arc<Mutex<HashSet<isize>>>,
    pub store: Arc<Mutex<MediaStore>>,
}

impl CaptureService {
    pub fn start(notify_hwnd: isize) -> Self {
        let (tx, rx) = mpsc::channel::<CaptureJob>();
        let store = Arc::new(Mutex::new(MediaStore::default()));
        let in_flight = Arc::new(Mutex::new(HashSet::new()));

        let store_bg = Arc::clone(&store);
        let in_flight_bg = Arc::clone(&in_flight);

        thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            let wic = create_wic_factory();
            while let Ok(job) = rx.recv() {
                match job {
                    CaptureJob::Window { hwnd } => {
                        let captured = capture_window_bgra(HWND(hwnd as *mut _), SNAPSHOT_WIDTH);
                        if let Ok(mut guard) = in_flight_bg.lock() {
                            guard.remove(&hwnd);
                        }
                        if let Some(img) = captured {
                            if let Ok(mut guard) = store_bg.lock() {
                                guard.keep_snapshot(hwnd, img, now_epoch_ms());
                            }
                            unsafe {
                                let _ = PostMessageW(
                                    HWND(notify_hwnd as *mut _),
                                    WM_FATHOM_CAPTURE_READY,
                                    WPARAM(hwnd as usize),
                                    LPARAM(0),
                                );
                            }
                        }
                    }
                    CaptureJob::Wallpaper { path, max_w, max_h } => {
                        if let Some(ref factory) = wic {
                            if let Some(img) = decode_image_file(factory, &path, max_w, max_h) {
                                if let Ok(mut guard) = store_bg.lock() {
                                    guard.wallpaper = Some((path, img));
                                }
                                unsafe {
                                    let _ = PostMessageW(
                                        HWND(notify_hwnd as *mut _),
                                        WM_FATHOM_WALLPAPER_READY,
                                        WPARAM(0),
                                        LPARAM(0),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            unsafe {
                CoUninitialize();
            }
        });

        Self { tx, in_flight, store }
    }

    pub fn request_window(&self, hwnd: isize, now_ms: u64) {
        let wants = self
            .store
            .lock()
            .map(|s| s.wants_snapshot(hwnd, now_ms))
            .unwrap_or(false);
        if !wants {
            return;
        }
        if let Ok(mut guard) = self.in_flight.lock() {
            if !guard.insert(hwnd) {
                return;
            }
        }
        let _ = self.tx.send(CaptureJob::Window { hwnd });
    }

    pub fn request_wallpaper(&self, path: &str, max_w: u32, max_h: u32) -> bool {
        let already = self
            .store
            .lock()
            .ok()
            .and_then(|s| s.wallpaper.as_ref().map(|(p, _)| p == path))
            .unwrap_or(false);
        if already {
            return true;
        }
        let _ = self.tx.send(CaptureJob::Wallpaper {
            path: path.to_string(),
            max_w,
            max_h,
        });
        false
    }

    pub fn ensure_icon_sync(&self, wic: Option<&IWICImagingFactory>, hwnd: isize, exe_path: &str) {
        let cached = self
            .store
            .lock()
            .map(|s| s.icons.contains_key(&hwnd))
            .unwrap_or(false);
        if cached {
            return;
        }
        let icon = wic.and_then(|f| decode_window_icon(f, HWND(hwnd as *mut _), exe_path));
        if let Ok(mut guard) = self.store.lock() {
            guard.icons.insert(hwnd, icon);
        }
    }
}

pub fn create_wic_factory() -> Option<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

pub fn decode_window_icon(
    wic: &IWICImagingFactory,
    hwnd: HWND,
    exe_path: &str,
) -> Option<BgraImage> {
    let (hicon, owned) = win_platform::extract_window_hicon(hwnd, exe_path)?;
    let img = hicon_to_bgra(wic, hicon);
    if owned {
        unsafe {
            let _ = DestroyIcon(hicon);
        }
    }
    img
}

fn hicon_to_bgra(wic: &IWICImagingFactory, hicon: HICON) -> Option<BgraImage> {
    unsafe {
        let bmp = wic.CreateBitmapFromHICON(hicon).ok()?;
        let converter = wic.CreateFormatConverter().ok()?;
        converter
            .Initialize(
                &bmp,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;
        let mut w = 0u32;
        let mut h = 0u32;
        converter.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let stride = w * 4;
        let mut buf = vec![0u8; (stride * h) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut buf).ok()?;
        Some(BgraImage {
            width: w,
            height: h,
            bgra_premul: Arc::new(buf),
        })
    }
}

fn decode_image_file(
    wic: &IWICImagingFactory,
    path: &str,
    max_w: u32,
    max_h: u32,
) -> Option<BgraImage> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let decoder = wic
            .CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                windows::Win32::Foundation::GENERIC_READ,
                WICDecodeMetadataCacheOnLoad,
            )
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let mut src_w = 0u32;
        let mut src_h = 0u32;
        frame.GetSize(&mut src_w, &mut src_h).ok()?;
        if src_w == 0 || src_h == 0 {
            return None;
        }

        let scale = ((max_w as f64 / src_w as f64).max(max_h as f64 / src_h as f64)).min(1.0);
        let dst_w = ((src_w as f64 * scale).round() as u32).max(1);
        let dst_h = ((src_h as f64 * scale).round() as u32).max(1);

        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(
                &frame,
                dst_w,
                dst_h,
                windows::Win32::Graphics::Imaging::WICBitmapInterpolationModeFant,
            )
            .ok()?;

        let converter = wic.CreateFormatConverter().ok()?;
        converter
            .Initialize(
                &scaler,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;

        let stride = dst_w * 4;
        let mut buf = vec![0u8; (stride * dst_h) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut buf).ok()?;
        Some(BgraImage {
            width: dst_w,
            height: dst_h,
            bgra_premul: Arc::new(buf),
        })
    }
}

/// Captures a non-minimized window using `PrintWindow(PW_RENDERFULLCONTENT)` and downscales to `max_width`.
fn capture_window_bgra(hwnd: HWND, max_width: u32) -> Option<BgraImage> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let src_w = (rect.right - rect.left).max(0) as u32;
        let src_h = (rect.bottom - rect.top).max(0) as u32;
        if src_w < 16 || src_h < 16 || src_w > 8192 || src_h > 8192 {
            return None;
        }

        let screen_dc = GetDC(HWND::default());
        let src_dc = CreateCompatibleDC(screen_dc);
        let src_bmp = CreateCompatibleBitmap(screen_dc, src_w as i32, src_h as i32);
        if src_dc.0.is_null() || src_bmp.0.is_null() {
            if !src_bmp.0.is_null() {
                let _ = DeleteObject(src_bmp);
            }
            if !src_dc.0.is_null() {
                let _ = DeleteDC(src_dc);
            }
            let _ = ReleaseDC(HWND::default(), screen_dc);
            return None;
        }
        let old_src = SelectObject(src_dc, src_bmp);

        // PW_RENDERFULLCONTENT = 2 captures DirectComposition / GPU-accelerated windows reliably.
        let printed = PrintWindow(hwnd, src_dc, PRINT_WINDOW_FLAGS(2)).as_bool()
            || PrintWindow(hwnd, src_dc, PRINT_WINDOW_FLAGS(0)).as_bool();

        let result = if printed {
            let scale = (max_width as f64 / src_w as f64).min(1.0);
            let dst_w = ((src_w as f64 * scale).round() as u32).max(1);
            let dst_h = ((src_h as f64 * scale).round() as u32).max(1);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: dst_w as i32,
                    biHeight: -(dst_h as i32), // top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let dst_dc = CreateCompatibleDC(screen_dc);
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let dst_bmp = CreateDIBSection(dst_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok();
            if let Some(dib) = dst_bmp {
                let old_dst = SelectObject(dst_dc, dib);
                SetStretchBltMode(dst_dc, HALFTONE);
                let blt_ok = if dst_w == src_w && dst_h == src_h {
                    BitBlt(dst_dc, 0, 0, dst_w as i32, dst_h as i32, src_dc, 0, 0, SRCCOPY).is_ok()
                } else {
                    StretchBlt(
                        dst_dc,
                        0,
                        0,
                        dst_w as i32,
                        dst_h as i32,
                        src_dc,
                        0,
                        0,
                        src_w as i32,
                        src_h as i32,
                        SRCCOPY,
                    )
                    .as_bool()
                };

                let out = if blt_ok && !bits.is_null() {
                    let byte_len = (dst_w * dst_h * 4) as usize;
                    let slice = std::slice::from_raw_parts(bits as *const u8, byte_len);
                    let mut buf = slice.to_vec();
                    // GDI leaves alpha byte = 0 on opaque windows; normalize to opaque premultiplied BGRA.
                    let mut any_non_black = false;
                    for px in buf.chunks_exact_mut(4) {
                        if px[0] != 0 || px[1] != 0 || px[2] != 0 {
                            any_non_black = true;
                        }
                        px[3] = 0xFF;
                    }
                    if any_non_black {
                        Some(BgraImage {
                            width: dst_w,
                            height: dst_h,
                            bgra_premul: Arc::new(buf),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                SelectObject(dst_dc, old_dst);
                let _ = DeleteObject(dib);
                let _ = DeleteDC(dst_dc);
                out
            } else {
                let _ = DeleteDC(dst_dc);
                None
            }
        } else {
            None
        };

        SelectObject(src_dc, old_src);
        let _ = DeleteObject(src_bmp);
        let _ = DeleteDC(src_dc);
        let _ = ReleaseDC(HWND::default(), screen_dc);
        result
    }
}
