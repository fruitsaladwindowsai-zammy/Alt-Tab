//! Hardware-accelerated Direct3D 11 + Direct2D + DirectWrite renderer for Fathom.
//!
//! Renders to both a live DXGI Flip-Discard swapchain and offscreen D3D11 textures (for
//! PNG verification renders). Computes interactive hit-test rectangles alongside drawing.

use crate::controller::{Controller, SwitchMode};
use crate::depth;
use crate::field::{self, WindowEntry, WorkspaceGroup};
use crate::layout::{self, OverlayMetrics, Rect};
use crate::palette::{DerivedPalette, Rgba};
use crate::win_capture::{BgraImage, MediaStore};
use std::collections::HashMap;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED,
    D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES,
    D2D1_BITMAP_PROPERTIES1, D2D1_BRUSH_PROPERTIES, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2,
    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_INTERPOLATION_MODE_LINEAR,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_ROUNDED_RECT, D2D1_UNIT_MODE_DIPS, D2D1CreateFactory, ID2D1Bitmap, ID2D1Bitmap1,
    ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1LinearGradientBrush, ID2D1RenderTarget,
    ID2D1RoundedRectangleGeometry, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
    D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteInlineObject, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2,
    IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::Graphics::Imaging::{
    GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
    WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::STGM;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
use windows::core::{HSTRING, Interface, PCWSTR};

#[derive(Clone, Debug, Default)]
pub struct HitRegions {
    pub gauge_rect: Option<Rect>,
    pub gauge_line_top: f64,
    pub gauge_line_bottom: f64,
    pub gauge_marks_x: f64,
    pub gauge_mark_size: f64,
    pub gauge_mark_step: f64,
    pub gauge_marks: Vec<layout::SoundingMark>,
    pub gauge_shown_indices: Vec<usize>,
    pub caption_rect: Option<Rect>,
    pub filter_rect: Option<Rect>,
    pub hints_rect: Option<Rect>,
    pub plane_rects: Vec<(usize, Rect)>,     // front-to-back
    pub workspace_cards: Vec<(usize, Rect)>, // index into controller.groups
    pub map_tiles: Vec<(usize, Rect)>,       // entry_idx, tile rect
    pub new_desktop_rect: Option<Rect>,
}

struct CachedBitmap {
    generation: u64,
    bitmap: ID2D1Bitmap1,
}

struct FontCache {
    dwrite: IDWriteFactory,
    family: String,
    ellipsis: Option<IDWriteInlineObject>,
}

impl FontCache {
    fn new(dwrite: IDWriteFactory, family: &str) -> Self {
        let dummy = create_raw_format(&dwrite, family, 12.0, false, false);
        let ellipsis = dummy
            .as_ref()
            .and_then(|fmt| unsafe { dwrite.CreateEllipsisTrimmingSign(fmt).ok() });
        Self {
            dwrite,
            family: family.to_string(),
            ellipsis,
        }
    }

    fn text_format(&self, size: f32, bold: bool, italic: bool) -> Option<IDWriteTextFormat> {
        let fmt = create_raw_format(&self.dwrite, &self.family, size.max(6.0), bold, italic)?;
        unsafe {
            let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if let Some(ref sign) = self.ellipsis {
                let trim = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                let _ = fmt.SetTrimming(&trim, sign);
            }
        }
        Some(fmt)
    }

    fn measure_width(&self, text: &str, size: f32, bold: bool, italic: bool) -> f32 {
        let Some(fmt) = self.text_format(size, bold, italic) else {
            return text.len() as f32 * size * 0.5;
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let Ok(layout) = self.dwrite.CreateTextLayout(&wide, &fmt, 4000.0, 200.0) else {
                return text.len() as f32 * size * 0.5;
            };
            let mut metrics = Default::default();
            if layout.GetMetrics(&mut metrics).is_ok() {
                metrics.widthIncludingTrailingWhitespace
            } else {
                0.0
            }
        }
    }

    fn create_layout(
        &self,
        text: &str,
        fmt: &IDWriteTextFormat,
        max_w: f32,
        max_h: f32,
    ) -> Option<IDWriteTextLayout> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.dwrite
                .CreateTextLayout(&wide, fmt, max_w.max(1.0), max_h.max(1.0))
                .ok()
        }
    }
}

fn create_raw_format(
    dwrite: &IDWriteFactory,
    family: &str,
    size: f32,
    bold: bool,
    italic: bool,
) -> Option<IDWriteTextFormat> {
    let fam = HSTRING::from(family);
    let locale = HSTRING::from("en-US");
    let weight = if bold {
        DWRITE_FONT_WEIGHT_BOLD
    } else {
        DWRITE_FONT_WEIGHT_NORMAL
    };
    let style = if italic {
        DWRITE_FONT_STYLE_ITALIC
    } else {
        DWRITE_FONT_STYLE_NORMAL
    };
    unsafe {
        dwrite
            .CreateTextFormat(
                &fam,
                None,
                weight,
                style,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &locale,
            )
            .ok()
    }
}

pub struct Renderer {
    d3d_device: ID3D11Device,
    d3d_ctx: ID3D11DeviceContext,
    d2d_factory: ID2D1Factory1,
    #[allow(dead_code)]
    d2d_device: ID2D1Device,
    dc: ID2D1DeviceContext,
    rt: ID2D1RenderTarget,
    dwrite: IDWriteFactory,
    fonts: FontCache,

    swap_chain: Option<IDXGISwapChain1>,
    width: u32,
    height: u32,

    snapshot_bitmaps: HashMap<isize, CachedBitmap>,
    icon_bitmaps: HashMap<isize, ID2D1Bitmap1>,
    wallpaper_bitmap: Option<(String, ID2D1Bitmap1)>,
}

impl Renderer {
    pub fn new(font_family: &str) -> windows::core::Result<Self> {
        unsafe {
            let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            let levels = [
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_0,
            ];
            let mut d3d_device = None;
            let mut d3d_ctx = None;
            let res = D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                flags,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                Some(&mut d3d_ctx),
            );
            if res.is_err() {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    None,
                    flags,
                    Some(&levels),
                    D3D11_SDK_VERSION,
                    Some(&mut d3d_device),
                    None,
                    Some(&mut d3d_ctx),
                )?;
            }
            let d3d_device = d3d_device.unwrap();
            let d3d_ctx = d3d_ctx.unwrap();

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dxgi_device: IDXGIDevice = d3d_device.cast()?;
            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
            let dc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            dc.SetDpi(96.0, 96.0);
            dc.SetUnitMode(D2D1_UNIT_MODE_DIPS);
            let rt: ID2D1RenderTarget = dc.cast()?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = FontCache::new(dwrite.clone(), font_family);

            Ok(Self {
                d3d_device,
                d3d_ctx,
                d2d_factory,
                d2d_device,
                dc,
                rt,
                dwrite,
                fonts,
                swap_chain: None,
                width: 0,
                height: 0,
                snapshot_bitmaps: HashMap::new(),
                icon_bitmaps: HashMap::new(),
                wallpaper_bitmap: None,
            })
        }
    }

    pub fn update_font_family(&mut self, family: &str) {
        if self.fonts.family != family {
            self.fonts = FontCache::new(self.dwrite.clone(), family);
        }
    }

    pub fn ensure_swap_chain(
        &mut self,
        hwnd: HWND,
        width: u32,
        height: u32,
    ) -> windows::core::Result<()> {
        let w = width.max(1);
        let h = height.max(1);
        unsafe {
            if let Some(ref sc) = self.swap_chain {
                if self.width == w && self.height == h {
                    return Ok(());
                }
                self.dc.SetTarget(None);
                sc.ResizeBuffers(2, w, h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SWAP_CHAIN_FLAG(0))?;
                self.width = w;
                self.height = h;
                return self.bind_swapchain_target();
            }

            let dxgi_device: IDXGIDevice = self.d3d_device.cast()?;
            let adapter = dxgi_device.GetAdapter()?;
            let factory: IDXGIFactory2 = adapter.GetParent()?;

            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: w,
                Height: h,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };
            let sc = factory.CreateSwapChainForHwnd(&self.d3d_device, hwnd, &desc, None, None)?;
            self.swap_chain = Some(sc);
            self.width = w;
            self.height = h;
            self.bind_swapchain_target()
        }
    }

    fn bind_swapchain_target(&self) -> windows::core::Result<()> {
        let Some(ref sc) = self.swap_chain else {
            return Ok(());
        };
        unsafe {
            let back_buf: IDXGISurface = sc.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: Default::default(),
            };
            let target: ID2D1Bitmap1 = self
                .dc
                .CreateBitmapFromDxgiSurface(&back_buf, Some(&props))?;
            self.dc.SetTarget(&target);
        }
        Ok(())
    }

    pub fn sync_media(&mut self, store: &MediaStore) {
        for (&hwnd, entry) in &store.snapshots {
            let needs = self
                .snapshot_bitmaps
                .get(&hwnd)
                .map(|c| c.generation != entry.generation)
                .unwrap_or(true);
            if needs && let Some(bmp) = self.upload_bgra(&entry.image) {
                self.snapshot_bitmaps.insert(
                    hwnd,
                    CachedBitmap {
                        generation: entry.generation,
                        bitmap: bmp,
                    },
                );
            }
        }
        self.snapshot_bitmaps
            .retain(|k, _| store.snapshots.contains_key(k));

        for (&hwnd, opt_img) in &store.icons {
            if let Some(img) = opt_img
                && !self.icon_bitmaps.contains_key(&hwnd)
                && let Some(bmp) = self.upload_bgra(img)
            {
                self.icon_bitmaps.insert(hwnd, bmp);
            }
        }
        self.icon_bitmaps.retain(|k, _| store.icons.contains_key(k));

        if let Some((ref path, ref img)) = store.wallpaper {
            let needs = self
                .wallpaper_bitmap
                .as_ref()
                .map(|(p, _)| p != path)
                .unwrap_or(true);
            if needs && let Some(bmp) = self.upload_bgra(img) {
                self.wallpaper_bitmap = Some((path.clone(), bmp));
            }
        }
    }

    fn upload_bgra(&self, img: &BgraImage) -> Option<ID2D1Bitmap1> {
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: Default::default(),
            colorContext: Default::default(),
        };
        unsafe {
            self.dc
                .CreateBitmap(
                    D2D_SIZE_U {
                        width: img.width,
                        height: img.height,
                    },
                    Some(img.bgra_premul.as_ptr() as *const _),
                    img.width * 4,
                    &props,
                )
                .ok()
        }
    }

    pub fn render_to_hwnd(
        &mut self,
        hwnd: HWND,
        ctrl: &Controller,
        store: &MediaStore,
        now_ms: u64,
    ) -> windows::core::Result<HitRegions> {
        let w = ctrl.target_monitor.width.round().max(1.0) as u32;
        let h = ctrl.target_monitor.height.round().max(1.0) as u32;
        self.ensure_swap_chain(hwnd, w, h)?;
        self.sync_media(store);
        let hits = self.draw_scene(ctrl, store, w as f64, h as f64, now_ms)?;
        if let Some(ref sc) = self.swap_chain {
            unsafe {
                let _ = sc.Present(1, DXGI_PRESENT(0));
            }
        }
        Ok(hits)
    }

    /// Renders offscreen at `width x height` and saves as PNG via WIC (for visual QA).
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_png(
        &mut self,
        wic: &IWICImagingFactory,
        out_path: &str,
        ctrl: &Controller,
        store: &MediaStore,
        width: u32,
        height: u32,
        now_ms: u64,
    ) -> windows::core::Result<()> {
        self.sync_media(store);
        unsafe {
            let tex_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut tex: Option<ID3D11Texture2D> = None;
            self.d3d_device
                .CreateTexture2D(&tex_desc, None, Some(&mut tex))?;
            let tex = tex.unwrap();

            let surface: IDXGISurface = tex.cast()?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: Default::default(),
            };
            let target: ID2D1Bitmap1 = self
                .dc
                .CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.dc.SetTarget(&target);
            self.draw_scene(ctrl, store, width as f64, height as f64, now_ms)?;
            self.dc.SetTarget(None);

            let mut staging_desc = tex_desc;
            staging_desc.Usage = D3D11_USAGE_STAGING;
            staging_desc.BindFlags = 0;
            staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging: Option<ID3D11Texture2D> = None;
            self.d3d_device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            let staging = staging.unwrap();
            self.d3d_ctx.CopyResource(&staging, &tex);

            let mut mapped = Default::default();
            self.d3d_ctx
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for y in 0..height {
                let src_row = (mapped.pData as *const u8).add((y * mapped.RowPitch) as usize);
                let dst_row = &mut pixels[(y * width * 4) as usize..((y + 1) * width * 4) as usize];
                std::ptr::copy_nonoverlapping(src_row, dst_row.as_mut_ptr(), (width * 4) as usize);
                for px in dst_row.as_chunks_mut::<4>().0 {
                    px[3] = 255;
                }
            }
            self.d3d_ctx.Unmap(&staging, 0);

            let stream = wic.CreateStream()?;
            let wide_path: Vec<u16> = out_path.encode_utf16().chain(std::iter::once(0)).collect();
            stream.InitializeFromFilename(PCWSTR(wide_path.as_ptr()), 0x40000000)?; // GENERIC_WRITE
            let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut frame = None;
            encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
            let frame = frame.unwrap();
            frame.Initialize(None)?;
            frame.SetSize(width, height)?;
            let mut fmt = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut fmt)?;
            frame.WritePixels(height, width * 4, &pixels)?;
            frame.Commit()?;
            encoder.Commit()?;
        }
        Ok(())
    }

    fn draw_scene(
        &self,
        ctrl: &Controller,
        store: &MediaStore,
        width: f64,
        height: f64,
        now_ms: u64,
    ) -> windows::core::Result<HitRegions> {
        let mut hits = HitRegions::default();
        let m = layout::overlay_metrics(width, height, ctrl.config.font_scale);
        let theme = &ctrl.palette;
        let corner_radius = ctrl.config.corner_radius;
        let border_width = ctrl.config.border_width;
        let master_alpha = ctrl.reveal_opacity.current.clamp(0.0, 1.0) as f32;
        let scene_depth = ctrl.scene_depth.current.clamp(0.0, 8.0);

        unsafe {
            self.dc.BeginDraw();
            self.dc.Clear(Some(&to_d2d_color(theme.background, 1.0)));

            // 1. Wallpaper (PreserveAspectCrop)
            if let Some((_, ref wp)) = self.wallpaper_bitmap {
                let wp_alpha = ctrl.wallpaper_opacity.current.clamp(0.0, 1.0) as f32 * master_alpha;
                if wp_alpha > 0.01 {
                    let bmp_ref: ID2D1Bitmap = wp.cast()?;
                    let sz = bmp_ref.GetSize();
                    let dest = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: width as f32,
                        bottom: height as f32,
                    };
                    let src = crop_src_rect(sz.width, sz.height, width as f32, height as f32);
                    self.dc.DrawBitmap(
                        &bmp_ref,
                        Some(&dest),
                        wp_alpha,
                        D2D1_INTERPOLATION_MODE_LINEAR,
                        Some(&src),
                        None,
                    );
                }
            }

            // 2. Themed vertical veil (top 0%, middle 55%, bottom 100%)
            self.fill_linear_gradient_3(
                rect_f(0.0, 0.0, width, height),
                theme.veil_top,
                theme.veil_middle,
                0.55,
                theme.veil_bottom,
                master_alpha,
            )?;

            // 3. Surface light from the top, fading as selection dives deeper
            let light_opacity = ((1.0 - scene_depth / 8.0).clamp(0.0, 1.0) as f32) * master_alpha;
            if light_opacity > 0.005 {
                self.fill_linear_gradient_2(
                    rect_f(0.0, 0.0, width, height * 0.45),
                    theme.surface_light,
                    theme.surface_light_end,
                    light_opacity,
                )?;
            }

            // 4. Depth shade dimming (3.5% per fathom)
            let shade_opacity = (scene_depth * 0.035) as f32 * master_alpha;
            if shade_opacity > 0.005 {
                self.fill_rect(
                    rect_f(0.0, 0.0, width, height),
                    theme.depth_shade,
                    shade_opacity,
                )?;
            }

            // 5. The Deep (back-to-front)
            self.draw_deep(
                ctrl,
                store,
                &m,
                theme,
                corner_radius,
                border_width,
                master_alpha,
                now_ms,
                &mut hits,
            )?;

            // 6. Sounding Line (left column when width >= 1000)
            if m.gauge_width > 0.0 {
                self.draw_sounding_line(ctrl, &m, theme, master_alpha, &mut hits)?;
            }

            // 7. Caption & Counter (or empty filter state)
            self.draw_caption(ctrl, &m, theme, master_alpha, &mut hits)?;

            // 8. Top-left Summary or Filter Bar
            self.draw_top_bar(
                ctrl,
                &m,
                theme,
                corner_radius,
                border_width,
                master_alpha,
                &mut hits,
            )?;

            // 9. Key Hints Row (bottom-right above map)
            self.draw_hints(
                ctrl,
                &m,
                theme,
                corner_radius,
                border_width,
                master_alpha,
                &mut hits,
            )?;

            // 10. Workspace Map ("The Surface") along the bottom
            self.draw_workspace_map(
                ctrl,
                store,
                &m,
                theme,
                corner_radius,
                border_width,
                master_alpha,
                &mut hits,
            )?;

            // 11. Floating drag-and-drop preview pill
            self.draw_drag_ghost(ctrl, &m, theme, corner_radius, master_alpha)?;

            let _ = self.dc.EndDraw(None, None);
        }

        Ok(hits)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_deep(
        &self,
        ctrl: &Controller,
        store: &MediaStore,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        border_width: f64,
        master_alpha: f32,
        now_ms: u64,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        let mut visible: Vec<(usize, layout::PlaneGeometry)> = Vec::new();
        for &entry_idx in &ctrl.order {
            let slot = ctrl.slots[entry_idx];
            if slot < 0 {
                continue;
            }
            let r = slot as f64 - ctrl.camera_slot.current;
            if let Some(geom) = layout::deep_plane(m.stage, r)
                && geom.opacity > 0.01
            {
                visible.push((entry_idx, geom));
            }
        }
        // Sort ascending by z so back cards render first and front cards occlude them cleanly.
        visible.sort_by(|a, b| {
            a.1.z
                .partial_cmp(&b.1.z)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for &(entry_idx, geom) in visible.iter().rev() {
            let left = geom.x - geom.width / 2.0;
            let top = geom.y - geom.height / 2.0;
            hits.plane_rects.push((
                entry_idx,
                Rect {
                    x: left,
                    y: top,
                    width: geom.width,
                    height: geom.height,
                },
            ));
        }

        for (entry_idx, geom) in visible {
            let entry = &ctrl.field[entry_idx];
            let slot = ctrl.slots[entry_idx];
            let r = slot as f64 - ctrl.camera_slot.current;
            let selected = ctrl.selected_index == Some(entry_idx);
            let hovered = ctrl.hovered_plane == Some(entry_idx);
            let card_alpha = geom.opacity as f32 * master_alpha;

            let left = geom.x - geom.width / 2.0;
            let top = geom.y - geom.height / 2.0;
            let card_rect = rect_f(left, top, geom.width, geom.height);

            let depth_scale = geom.scale.min(1.0);
            let radius = corner_radius * depth_scale;
            let pad = (8.0 * m.unit * depth_scale).max(3.0);
            let header_h = (geom.height * 0.1).clamp(18.0 * m.text_unit, 30.0 * m.text_unit);
            let fog = theme.fog_strength as f64
                * (r.clamp(0.0, 1.0) * (0.12 + depth::fog_for_depth(entry.depth) * 0.75)
                    + 0.05 * (r - 1.0).max(0.0))
                .min(0.70);

            // Soft layered shadow / selected accent glow
            let shadow_color = if selected { theme.glow } else { theme.shadow };
            let shadow_blur = if selected {
                36.0 * m.unit
            } else {
                22.0 * m.unit * depth_scale
            };
            let shadow_spread = if selected { 3.0 * m.unit } else { 0.0 };
            let shadow_off_y = if selected {
                0.0
            } else {
                8.0 * m.unit * depth_scale
            };
            self.draw_soft_shadow(
                card_rect,
                radius as f32,
                shadow_color,
                shadow_blur as f32,
                shadow_spread as f32,
                shadow_off_y as f32,
                card_alpha,
            )?;

            // Card surface gradient + border
            let (top_col, bot_col) = if selected {
                (theme.card_selected_top, theme.card_selected_bottom)
            } else {
                (theme.card_top, theme.card_bottom)
            };
            self.fill_rounded_gradient_2(card_rect, radius as f32, top_col, bot_col, card_alpha)?;
            let border_col = if hovered {
                theme.card_border_hover
            } else {
                theme.card_border
            };
            self.draw_rounded_rect(
                card_rect,
                radius as f32,
                border_col,
                border_width as f32,
                card_alpha,
            )?;

            // Header strip: icon, title, short age
            let header_alpha = card_alpha * (1.0 - fog as f32 * 0.45);
            let hx = left + pad + 2.0 * m.unit;
            let hw = (geom.width - (pad + 2.0 * m.unit) * 2.0).max(1.0);
            let icon_sz = header_h * 0.6;
            let icon_y = top + (header_h - icon_sz) / 2.0;
            self.draw_app_icon(entry, hx, icon_y, icon_sz, header_alpha)?;

            let age_text = field::age_short(entry.seconds, entry.active, entry.estimated);
            let max_age_sz = (11.0 * m.text_unit).max(7.0);
            let age_size = (header_h * 0.4).clamp(7.0, max_age_sz) as f32;
            let age_w = if age_text.is_empty() {
                0.0
            } else {
                self.fonts.measure_width(&age_text, age_size, false, false) as f64 + 8.0 * m.unit
            };

            let title_x = hx + icon_sz + 7.0 * m.unit;
            let title_w = (hw - icon_sz - 7.0 * m.unit - age_w).max(1.0);
            let max_title_sz = (13.0 * m.text_unit).max(8.0);
            let title_size = (header_h * 0.46).clamp(8.0, max_title_sz) as f32;
            let title_str = if entry.title.is_empty() {
                &entry.app_name
            } else {
                &entry.title
            };
            self.draw_text(
                title_str,
                rect_f(title_x, top, title_w, header_h),
                title_size,
                selected,
                false,
                theme.text,
                header_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;

            if !age_text.is_empty() {
                self.draw_text(
                    &age_text,
                    rect_f(hx + hw - age_w, top, age_w, header_h),
                    age_size,
                    false,
                    false,
                    theme.text_soft,
                    header_alpha,
                    DWRITE_TEXT_ALIGNMENT_TRAILING,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                )?;
            }

            // Inner preview area
            let preview_x = left + pad;
            let preview_y = top + header_h;
            let preview_w = (geom.width - pad * 2.0).max(1.0);
            let preview_h = (geom.height - header_h - pad).max(1.0);

            let cached_snap = self.snapshot_bitmaps.get(&entry.hwnd);
            let aspect = if let Some(c) = cached_snap {
                let sz = unsafe { c.bitmap.GetSize() };
                if sz.height > 0.0 {
                    (sz.width / sz.height) as f64
                } else {
                    1.6
                }
            } else if let Some(g) = entry.geometry {
                if g.height > 0.0 {
                    (g.width / g.height).clamp(0.3, 5.0)
                } else {
                    1.6
                }
            } else {
                1.6
            };
            let fitted = layout::fit(preview_w, preview_h, aspect);
            let frame_rect = rect_f(
                preview_x + fitted.x,
                preview_y + fitted.y,
                fitted.width,
                fitted.height,
            );
            let frame_radius = (radius * 0.55).max(0.0) as f32;

            // Clipped frame content
            self.push_rounded_clip(frame_rect, frame_radius)?;
            self.fill_rect(frame_rect, theme.bed, card_alpha)?;

            if let Some(c) = cached_snap {
                let bmp_ref: ID2D1Bitmap = c.bitmap.cast()?;
                let sz = unsafe { bmp_ref.GetSize() };
                let src = crop_src_rect(
                    sz.width,
                    sz.height,
                    fitted.width as f32,
                    fitted.height as f32,
                );
                unsafe {
                    self.dc.DrawBitmap(
                        &bmp_ref,
                        Some(&frame_rect),
                        card_alpha,
                        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                        Some(&src),
                        None,
                    );
                }
            } else {
                // App icon + name fallback
                let big_icon = (fitted.height * 0.3).min(88.0 * m.unit).max(16.0);
                let show_app = fitted.height > 110.0 * m.unit;
                let show_status = (entry.minimized || !entry.on_current_desktop)
                    && fitted.height > 130.0 * m.unit;
                let mut total_h = big_icon;
                if show_app {
                    total_h += 8.0 * m.unit + 18.0 * m.text_unit;
                }
                if show_status {
                    total_h += 6.0 * m.unit + 14.0 * m.text_unit;
                }
                let start_y = frame_rect.top as f64 + (fitted.height - total_h) / 2.0;
                let icon_x = frame_rect.left as f64 + (fitted.width - big_icon) / 2.0;
                self.draw_app_icon(entry, icon_x, start_y, big_icon, card_alpha)?;

                if show_app {
                    let app_sz = (fitted.height * 0.05).min(13.0 * m.text_unit).max(10.0) as f32;
                    self.draw_text(
                        &entry.app_name,
                        rect_f(
                            frame_rect.left as f64 + 10.0 * m.unit,
                            start_y + big_icon + 8.0 * m.unit,
                            (fitted.width - 20.0 * m.unit).max(1.0),
                            20.0 * m.text_unit,
                        ),
                        app_sz,
                        false,
                        false,
                        theme.text_soft,
                        card_alpha,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                    )?;
                }
                if show_status {
                    let sub_msg = if entry.minimized {
                        "minimized · no live preview"
                    } else {
                        "other desktop · no live preview"
                    };
                    let sub_sz = (fitted.height * 0.042).min(11.0 * m.text_unit).max(9.0) as f32;
                    self.draw_text(
                        sub_msg,
                        rect_f(
                            frame_rect.left as f64 + 10.0 * m.unit,
                            start_y + big_icon + 28.0 * m.unit,
                            (fitted.width - 20.0 * m.unit).max(1.0),
                            18.0 * m.text_unit,
                        ),
                        sub_sz,
                        false,
                        false,
                        theme.text_faint,
                        card_alpha,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                    )?;
                }
            }

            // Fog layer inside frame (not over header)
            let fog_alpha = fog as f32 * card_alpha;
            if fog_alpha > 0.005 {
                self.fill_rect(frame_rect, theme.fog, fog_alpha)?;
            }

            unsafe {
                self.dc.PopLayer();
            }

            // Frame outline
            self.draw_rounded_rect(
                frame_rect,
                frame_radius,
                theme.frame_edge,
                border_width as f32,
                card_alpha,
            )?;

            // "last seen X" badge for minimized or off-desktop snapshots
            if cached_snap.is_some()
                && (entry.minimized || !entry.on_current_desktop)
                && fitted.height > 70.0 * m.unit
                && let Some(snap_entry) = store.snapshots.get(&entry.hwnd)
            {
                let secs = (now_ms.saturating_sub(snap_entry.taken_at_ms)) as f64 / 1000.0;
                let badge = format!("last seen {}", field::age_label(secs, false, false));
                let bsz = (11.0 * m.text_unit * depth_scale.max(0.8)).max(9.0) as f32;
                let bw = self.fonts.measure_width(&badge, bsz, false, false) as f64 + 14.0 * m.unit;
                let bh = bsz as f64 + 10.0 * m.unit;
                let bm = 8.0 * m.unit * depth_scale + 2.0;
                let br = rect_f(
                    frame_rect.left as f64 + bm,
                    frame_rect.bottom as f64 - bm - bh,
                    bw,
                    bh,
                );
                let brad = corner_radius.min(bh / 2.0) as f32;
                self.fill_rounded_rect(br, brad, theme.panel, card_alpha)?;
                self.draw_rounded_rect(
                    br,
                    brad,
                    theme.panel_border,
                    border_width as f32,
                    card_alpha,
                )?;
                self.draw_text(
                    &badge,
                    br,
                    bsz,
                    false,
                    false,
                    theme.text_soft,
                    card_alpha,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                )?;
            }

            // Selection ring or urgent border
            if selected || entry.urgent {
                let inset = if selected { -3.0 * m.unit } else { 0.0 };
                let ring_r = if selected && radius > 0.0 {
                    radius + 3.0 * m.unit
                } else {
                    radius
                };
                let ring_rect = rect_f(
                    left + inset,
                    top + inset,
                    geom.width - inset * 2.0,
                    geom.height - inset * 2.0,
                );
                let ring_w = if selected {
                    (border_width + 1.0).max(2.5 * m.unit)
                } else {
                    border_width.max(1.5)
                };
                let ring_col = if selected { theme.accent } else { theme.urgent };
                self.draw_rounded_rect(
                    ring_rect,
                    ring_r as f32,
                    ring_col,
                    ring_w as f32,
                    card_alpha,
                )?;
            }

            // Urgent red badge dot
            if entry.urgent {
                let dw = 12.0 * m.unit;
                let cx = left + geom.width - dw / 2.0 + 5.0 * m.unit;
                let cy = top + dw / 2.0 - 5.0 * m.unit;
                self.fill_circle(cx, cy, dw / 2.0, theme.urgent, card_alpha)?;
                self.draw_circle(cx, cy, dw / 2.0, theme.background, 2.0, card_alpha)?;
            }
        }

        Ok(())
    }

    fn draw_sounding_line(
        &self,
        ctrl: &Controller,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        let gx = m.margin;
        let gy = m.deep_top;
        let gw = m.gauge_width;
        let gh = (m.stage.front_y + m.stage.front_height / 2.0 - m.deep_top).max(40.0);

        let line_x = gx + 46.0 * m.text_unit;
        let line_top = gy + 22.0 * m.unit;
        let line_bottom = gy + gh - 8.0 * m.unit;
        let mark_size = 6.0 * m.unit;
        let mark_step = mark_size + 2.0 * m.unit;
        let marks_x = line_x + 8.0 * m.unit;
        let max_cols =
            (((gx + gw - marks_x - 18.0 * m.text_unit) / mark_step).floor() as usize).max(1);

        let shown: Vec<&WindowEntry> = ctrl.order.iter().map(|&idx| &ctrl.field[idx]).collect();
        let depths: Vec<f64> = shown.iter().map(|e| e.depth).collect();
        let marks =
            layout::sounding_marks(&depths, line_top, line_bottom, mark_size * 1.3, max_cols);

        hits.gauge_rect = Some(Rect {
            x: gx,
            y: gy,
            width: gw,
            height: gh,
        });
        hits.gauge_line_top = line_top;
        hits.gauge_line_bottom = line_bottom;
        hits.gauge_marks_x = marks_x;
        hits.gauge_mark_size = mark_size;
        hits.gauge_mark_step = mark_step;
        hits.gauge_marks = marks.clone();
        hits.gauge_shown_indices = ctrl.order.clone();

        // Surface "≈"
        self.draw_text(
            "≈",
            rect_f(
                line_x - 14.0 * m.text_unit,
                line_top - 22.0 * m.unit,
                28.0 * m.text_unit,
                18.0 * m.unit,
            ),
            (14.0 * m.text_unit) as f32,
            false,
            false,
            theme.text_faint,
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        // Main vertical line
        self.fill_rect(
            rect_f(
                line_x - 0.5,
                line_top,
                1.0,
                (line_bottom - line_top).max(1.0),
            ),
            theme.line,
            master_alpha,
        )?;

        // 9 fathom ticks & labels
        for i in 0..=8usize {
            let ty = layout::sounding_y(i as f64, line_top, line_bottom);
            let tw = (if i % 2 == 0 { 9.0 } else { 5.0 }) * m.unit;
            self.fill_rect(
                rect_f(line_x - tw / 2.0, ty - 0.5, tw, 1.0),
                theme.tick,
                master_alpha,
            )?;

            let lbl = field::depth_mark_label(i);
            self.draw_text(
                &lbl,
                rect_f(
                    gx,
                    ty - 8.0 * m.text_unit,
                    (line_x - 9.0 * m.unit - gx).max(1.0),
                    16.0 * m.text_unit,
                ),
                (10.0 * m.text_unit) as f32,
                false,
                false,
                theme.text_faint,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
        }

        // Window dots
        for (i, mark) in marks.iter().enumerate() {
            if mark.hidden {
                continue;
            }
            let entry = shown[i];
            let selected = ctrl.selected_index == Some(entry.index);
            let cx = marks_x + mark.column as f64 * mark_step + mark_size / 2.0;
            let cy = mark.y;
            let r = mark_size / 2.0;
            if selected {
                self.fill_circle(cx, cy, r, theme.accent, master_alpha)?;
            } else if entry.estimated {
                self.draw_circle(cx, cy, r, theme.mark_border, 1.0, master_alpha)?;
            } else {
                self.fill_circle(cx, cy, r, theme.mark, master_alpha)?;
            }
            if mark.more > 0 {
                let plus = format!("+{}", mark.more);
                self.draw_text(
                    &plus,
                    rect_f(
                        cx + r + 3.0 * m.unit,
                        cy - 7.0 * m.text_unit,
                        28.0 * m.text_unit,
                        14.0 * m.text_unit,
                    ),
                    (9.0 * m.text_unit) as f32,
                    false,
                    false,
                    theme.text_faint,
                    master_alpha,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                )?;
            }
        }

        // Sounding lead: accent line from surface + rotated diamond
        if ctrl.selected_index.is_some() {
            let lead_y = layout::sounding_y(ctrl.scene_depth.current, line_top, line_bottom);
            self.fill_rect(
                rect_f(line_x - 0.75, line_top, 1.5, (lead_y - line_top).max(0.0)),
                theme.accent,
                master_alpha,
            )?;
            let ds = (9.0 * m.unit) as f32;
            self.draw_diamond(
                line_x as f32,
                lead_y as f32,
                ds,
                1.5,
                theme.accent,
                master_alpha,
            )?;
        }

        Ok(())
    }

    fn draw_caption(
        &self,
        ctrl: &Controller,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        if ctrl.opened && ctrl.order.is_empty() {
            let cy = (m.height - m.map_height) / 2.0;
            let msg = format!("No window matches “{}”", ctrl.filter_text);
            self.draw_text(
                &msg,
                rect_f(
                    m.margin,
                    cy - 26.0 * m.unit,
                    m.width - m.margin * 2.0,
                    28.0 * m.text_unit,
                ),
                (20.0 * m.text_unit) as f32,
                false,
                false,
                theme.text,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
            self.draw_text(
                "Backspace to edit  ·  Esc to clear",
                rect_f(
                    m.margin,
                    cy + 8.0 * m.unit,
                    m.width - m.margin * 2.0,
                    20.0 * m.text_unit,
                ),
                (13.0 * m.text_unit) as f32,
                false,
                false,
                theme.text_soft,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
            return Ok(());
        }

        let Some(entry) = ctrl.selected_entry() else {
            return Ok(());
        };

        let cx = m.stage.front_x - m.stage.front_width / 2.0;
        let cy = m.height - m.margin - m.map_height - m.caption_height - 14.0 * m.unit;
        let cw = (m.width - cx - m.margin).max(40.0);
        hits.caption_rect = Some(Rect {
            x: cx,
            y: cy,
            width: cw,
            height: m.caption_height,
        });

        let icon_sz = 38.0 * m.unit;
        let icon_y = cy + (m.caption_height - icon_sz) / 2.0;
        self.draw_app_icon(entry, cx, icon_y, icon_sz, master_alpha)?;

        let counter_str = if ctrl.selected_slot() >= 0 {
            format!("{} / {}", ctrl.selected_slot() + 1, ctrl.order.len())
        } else {
            String::new()
        };
        let counter_sz = (13.0 * m.text_unit) as f32;
        let counter_w = self
            .fonts
            .measure_width(&counter_str, counter_sz, false, false) as f64
            + 8.0 * m.unit;

        self.draw_text(
            &counter_str,
            rect_f(cx + cw - counter_w, cy, counter_w, m.caption_height),
            counter_sz,
            false,
            false,
            theme.text_soft,
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        let text_x = cx + icon_sz + 14.0 * m.unit;
        let text_w = (cw - icon_sz - 14.0 * m.unit - counter_w - 20.0 * m.unit).max(20.0);

        let title_sz = (20.0 * m.text_unit) as f32;
        let sub_sz = (13.0 * m.text_unit) as f32;
        let title_h = 26.0 * m.text_unit;
        let sub_h = 18.0 * m.text_unit;
        let col_top = cy + (m.caption_height - title_h - sub_h - 3.0 * m.unit) / 2.0;

        let title_str = if entry.title.is_empty() {
            &entry.app_name
        } else {
            &entry.title
        };
        self.draw_text(
            title_str,
            rect_f(text_x, col_top, text_w, title_h),
            title_sz,
            true,
            false,
            theme.text,
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        let mut parts = Vec::new();
        if !entry.app_name.is_empty() {
            parts.push(entry.app_name.clone());
        }
        let ws = field::workspace_label(&entry.workspace_name, entry.workspace_id);
        parts.push(if field::is_special_name(&entry.workspace_name) {
            format!("scratchpad {ws}")
        } else {
            format!("workspace {ws}")
        });
        parts.push(field::age_label(
            entry.seconds,
            entry.active,
            entry.estimated,
        ));
        let reading = field::fathom_label(entry.depth, entry.active, entry.estimated);
        if !reading.is_empty() {
            parts.push(reading);
        }
        if entry.urgent {
            parts.push("wants attention".to_string());
        }
        let sub = parts.join("  ·  ");
        self.draw_text(
            &sub,
            rect_f(text_x, col_top + title_h + 3.0 * m.unit, text_w, sub_h),
            sub_sz,
            false,
            false,
            theme.text_soft,
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_top_bar(
        &self,
        ctrl: &Controller,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        border_width: f64,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        let filtering = !ctrl.filter_text.is_empty();
        let show_filter = filtering || ctrl.mode == SwitchMode::Browse;

        if !show_filter {
            let win_count = ctrl.order.len();
            let ws_count = ctrl.shown_workspace_count();
            let summary = format!(
                "{} {}  ·  {} {}",
                win_count,
                if win_count == 1 { "window" } else { "windows" },
                ws_count,
                if ws_count == 1 {
                    "workspace"
                } else {
                    "workspaces"
                }
            );
            self.draw_text(
                &summary,
                rect_f(
                    m.margin,
                    m.margin + 8.0 * m.unit,
                    420.0 * m.text_unit,
                    20.0 * m.text_unit,
                ),
                (13.0 * m.text_unit) as f32,
                false,
                false,
                theme.text_soft,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            )?;
            return Ok(());
        }

        let sz = (14.0 * m.text_unit) as f32;
        let cnt_sz = (12.0 * m.text_unit) as f32;
        let slash_w = self.fonts.measure_width("/", sz, false, false) as f64;
        let query_text = if filtering {
            &ctrl.filter_text
        } else {
            "type to filter"
        };
        let query_w = self.fonts.measure_width(query_text, sz, false, false) as f64;
        let count_text = format!("{} of {}", ctrl.order.len(), ctrl.field.len());
        let count_w = if filtering {
            self.fonts.measure_width(&count_text, cnt_sz, false, false) as f64
        } else {
            0.0
        };

        let gap = 8.0 * m.unit;
        let mut row_w = slash_w + gap + query_w;
        if filtering {
            row_w += gap + 2.0 + gap + count_w;
        }
        let bar_h = 34.0 * m.unit;
        let bar_w = row_w + 28.0 * m.unit;
        let bar_rect = rect_f(m.margin, m.margin, bar_w, bar_h);
        let rad = corner_radius.min(bar_h / 2.0) as f32;

        hits.filter_rect = Some(Rect {
            x: m.margin,
            y: m.margin,
            width: bar_w,
            height: bar_h,
        });

        self.fill_rounded_rect(bar_rect, rad, theme.panel, master_alpha)?;
        self.draw_rounded_rect(
            bar_rect,
            rad,
            if filtering {
                theme.accent
            } else {
                theme.panel_border
            },
            border_width as f32,
            master_alpha,
        )?;

        let mut x = m.margin + 14.0 * m.unit;
        self.draw_text(
            "/",
            rect_f(x, m.margin, slash_w + 2.0, bar_h),
            sz,
            false,
            false,
            if filtering {
                theme.accent_text
            } else {
                theme.text_faint
            },
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;
        x += slash_w + gap;

        self.draw_text(
            query_text,
            rect_f(x, m.margin, query_w + 4.0, bar_h),
            sz,
            false,
            false,
            if filtering {
                theme.text
            } else {
                theme.text_faint
            },
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;
        x += query_w + gap;

        if filtering {
            self.fill_rect(
                rect_f(
                    x,
                    m.margin + (bar_h - 16.0 * m.unit) / 2.0,
                    2.0,
                    16.0 * m.unit,
                ),
                theme.accent,
                master_alpha,
            )?;
            x += 2.0 + gap;
            self.draw_text(
                &count_text,
                rect_f(x, m.margin, count_w + 4.0, bar_h),
                cnt_sz,
                false,
                false,
                theme.text_soft,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_hints(
        &self,
        ctrl: &Controller,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        border_width: f64,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        let holding = ctrl.mode == SwitchMode::Hold;
        let filtering = !ctrl.filter_text.is_empty();
        let pairs: [(&str, &str); 5] = if holding {
            [
                ("Tab", "deeper"),
                ("↑↓", "dive"),
                ("←→", "workspace"),
                ("Space", "keep open"),
                ("Esc", "cancel"),
            ]
        } else {
            [
                ("↑↓", "dive"),
                ("←→", "workspace"),
                ("1–9", "go to"),
                ("Enter", "focus"),
                ("Esc", if filtering { "clear" } else { "close" }),
            ]
        };

        let sz = (11.0 * m.text_unit) as f32;
        let pill_h = 18.0 * m.unit;
        let spacing_item = 16.0 * m.unit;
        let spacing_inner = 5.0 * m.unit;

        let mut total_w = 0.0;
        let mut widths = Vec::with_capacity(pairs.len());
        for (k, v) in pairs {
            let kw = self.fonts.measure_width(k, sz, false, false) as f64;
            let pill_w = (kw + 10.0 * m.unit).max(pill_h);
            let vw = self.fonts.measure_width(v, sz, false, false) as f64;
            let item_w = pill_w + spacing_inner + vw;
            widths.push((pill_w, vw));
            total_w += item_w;
        }
        total_w += spacing_item * (pairs.len() - 1) as f64;

        let y = m.height - m.margin - m.map_height - 20.0 * m.unit;
        let mut x = m.width - m.margin - total_w;
        hits.hints_rect = Some(Rect {
            x,
            y,
            width: total_w,
            height: pill_h,
        });

        for ((k, v), (pill_w, vw)) in pairs.into_iter().zip(widths) {
            let pr = rect_f(x, y, pill_w, pill_h);
            let rad = corner_radius.min(pill_h / 2.0) as f32;
            self.fill_rounded_rect(pr, rad, theme.key, master_alpha)?;
            self.draw_rounded_rect(pr, rad, theme.key_border, border_width as f32, master_alpha)?;
            self.draw_text(
                k,
                pr,
                sz,
                false,
                false,
                theme.text_soft,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;

            x += pill_w + spacing_inner;
            self.draw_text(
                v,
                rect_f(x, y, vw + 4.0, pill_h),
                sz,
                false,
                false,
                theme.text_faint,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
            x += vw + spacing_item;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_workspace_map(
        &self,
        ctrl: &Controller,
        _store: &MediaStore,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        border_width: f64,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        if ctrl.groups.is_empty() {
            return Ok(());
        }

        let map_x = m.margin;
        let map_w = (m.width - m.margin * 2.0).max(1.0);
        let map_y = m.height - m.margin - m.map_height;
        let gap = 10.0 * m.unit;
        let chrome = 16.0 * m.unit;
        let header_space = 14.0 * m.unit + 20.0 * m.text_unit;
        let multi_monitor = ctrl.monitors.len() > 1;
        let default_vp = ctrl
            .monitors
            .values()
            .next()
            .map(|d| d.rect)
            .unwrap_or(ctrl.target_monitor);

        let mut aspects = Vec::with_capacity(ctrl.groups.len());
        for group in &ctrl.groups {
            let vp = ctrl
                .monitors
                .get(&group.monitor)
                .map(|d| d.rect)
                .unwrap_or(default_vp);
            let win_geoms: Vec<Option<Rect>> = group
                .entries
                .iter()
                .filter_map(|&idx| ctrl.field[idx].geometry)
                .map(Some)
                .collect();
            let bounds = layout::minimap_bounds(vp, &win_geoms);
            aspects.push(bounds.width / bounds.height.max(1.0));
        }

        let plus_w = if ctrl.groups.len() >= 8 {
            76.0 * m.unit
        } else {
            100.0 * m.unit
        }
        .clamp(62.0, 120.0);
        let cards_avail_w = (map_w - plus_w - gap).max(90.0);

        let metrics = layout::map_cards(
            &aspects,
            cards_avail_w,
            (m.map_height - header_space).max(20.0),
            gap,
            chrome,
            84.0 * m.unit,
        );
        let cards_w: f64 = metrics.widths.iter().sum::<f64>()
            + gap * (metrics.widths.len().saturating_sub(1) as f64);
        let row_w = cards_w + gap + plus_w;
        let card_h = metrics.height + header_space;
        let card_top = map_y + m.map_height - card_h;
        let mut cursor_x = map_x + (map_w - row_w) / 2.0;

        for (g_idx, group) in ctrl.groups.iter().enumerate() {
            let card_w = metrics.widths[g_idx];
            let card_rect = rect_f(cursor_x, card_top, card_w, card_h);
            hits.workspace_cards.push((
                g_idx,
                Rect {
                    x: cursor_x,
                    y: card_top,
                    width: card_w,
                    height: card_h,
                },
            ));

            self.draw_workspace_card(
                ctrl,
                g_idx,
                group,
                card_rect,
                m,
                theme,
                corner_radius,
                border_width,
                multi_monitor,
                default_vp,
                master_alpha,
                hits,
            )?;

            cursor_x += card_w + gap;
        }

        // "+ New desktop" action card at the end of the workspace row
        let new_rect_px = Rect {
            x: cursor_x,
            y: card_top,
            width: plus_w,
            height: card_h,
        };
        hits.new_desktop_rect = Some(new_rect_px);
        let new_rect = rect_f(cursor_x, card_top, plus_w, card_h);
        let new_active = ctrl.drop_new_desktop || ctrl.hovered_new_desktop;
        let new_bg = if new_active {
            theme.map_card_selected
        } else {
            theme.map_card
        };
        let new_border = if new_active {
            theme.accent
        } else {
            theme.map_card_border
        };
        let new_bw = if ctrl.drop_new_desktop {
            (border_width + 1.5).max(2.0)
        } else if ctrl.hovered_new_desktop {
            (border_width + 0.5).max(1.5)
        } else {
            border_width
        };
        self.fill_rounded_rect(new_rect, corner_radius as f32, new_bg, master_alpha)?;
        self.draw_rounded_rect(
            new_rect,
            corner_radius as f32,
            new_border,
            new_bw as f32,
            master_alpha,
        )?;

        let plus_h = (card_h * 0.42).max(22.0);
        let plus_y = card_top + card_h * 0.22;
        self.draw_text(
            "+",
            rect_f(cursor_x, plus_y, plus_w, plus_h),
            (22.0 * m.text_unit) as f32,
            true,
            false,
            if new_active {
                theme.accent
            } else {
                theme.text_soft
            },
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;
        let sub_label = if ctrl.drop_new_desktop {
            "Drop here"
        } else if plus_w >= 86.0 * m.unit {
            "New desktop"
        } else {
            "New"
        };
        self.draw_text(
            sub_label,
            rect_f(
                cursor_x + 4.0,
                plus_y + plus_h - 2.0,
                plus_w - 8.0,
                18.0 * m.text_unit,
            ),
            (10.5 * m.text_unit) as f32,
            false,
            false,
            if new_active {
                theme.accent_text
            } else {
                theme.text_faint
            },
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_drag_ghost(
        &self,
        ctrl: &Controller,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        master_alpha: f32,
    ) -> windows::core::Result<()> {
        let Some(entry_idx) = ctrl.dragging_entry else {
            return Ok(());
        };
        let Some(entry) = ctrl.field.get(entry_idx) else {
            return Ok(());
        };

        let pill_w = (230.0 * m.unit).clamp(180.0, 280.0);
        let has_target = ctrl.drop_target_group.is_some() || ctrl.drop_new_desktop;
        let pill_h = if has_target {
            54.0 * m.unit
        } else {
            40.0 * m.unit
        };
        let px = (ctrl.drag_pos.0 - pill_w * 0.5).clamp(8.0, (m.width - pill_w - 8.0).max(8.0));
        let py = (ctrl.drag_pos.1 - pill_h - 10.0).clamp(8.0, (m.height - pill_h - 8.0).max(8.0));
        let r = rect_f(px, py, pill_w, pill_h);
        let rad = corner_radius.min(10.0) as f32;

        self.fill_rounded_rect(
            rect_f(px + 2.0, py + 4.0, pill_w, pill_h),
            rad,
            theme.shadow,
            0.7 * master_alpha,
        )?;
        self.fill_rounded_rect(r, rad, theme.panel, 0.96 * master_alpha)?;
        self.draw_rounded_rect(r, rad, theme.accent, 1.8, master_alpha)?;

        let pad = 10.0 * m.unit;
        let icon_sz = 18.0 * m.unit;
        if let Some(bmp) = self.icon_bitmaps.get(&entry.hwnd) {
            unsafe {
                let bmp_ref: ID2D1Bitmap = bmp.cast()?;
                let dest = rect_f(px + pad, py + 11.0 * m.unit, icon_sz, icon_sz);
                self.dc.DrawBitmap(
                    &bmp_ref,
                    Some(&dest),
                    master_alpha,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    None,
                    None,
                );
            }
        }
        let text_x = px + pad + icon_sz + 8.0 * m.unit;
        let text_w = (pill_w - (text_x - px) - pad).max(20.0);
        self.draw_text(
            &entry.title,
            rect_f(text_x, py + 9.0 * m.unit, text_w, 22.0 * m.text_unit),
            (12.5 * m.text_unit) as f32,
            true,
            false,
            theme.text,
            master_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        if has_target {
            let dest_msg = if ctrl.drop_new_desktop {
                "Move to + New desktop".to_string()
            } else if let Some(gi) = ctrl.drop_target_group {
                let name = ctrl
                    .groups
                    .get(gi)
                    .map(|g| g.label.as_str())
                    .unwrap_or("desktop");
                format!("Move to Desktop {name}")
            } else {
                String::new()
            };
            self.draw_text(
                &dest_msg,
                rect_f(text_x, py + 30.0 * m.unit, text_w, 16.0 * m.text_unit),
                (10.5 * m.text_unit) as f32,
                false,
                false,
                theme.accent_text,
                master_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_workspace_card(
        &self,
        ctrl: &Controller,
        g_idx: usize,
        group: &WorkspaceGroup,
        card_rect: D2D_RECT_F,
        m: &OverlayMetrics,
        theme: &DerivedPalette,
        corner_radius: f64,
        border_width: f64,
        multi_monitor: bool,
        default_vp: Rect,
        master_alpha: f32,
        hits: &mut HitRegions,
    ) -> windows::core::Result<()> {
        let card_x = card_rect.left as f64;
        let card_y = card_rect.top as f64;
        let card_w = (card_rect.right - card_rect.left) as f64;
        let card_h = (card_rect.bottom - card_rect.top) as f64;

        let shown_count = group
            .entries
            .iter()
            .filter(|&&idx| ctrl.slots.get(idx).copied().unwrap_or(-1) >= 0)
            .count();
        let card_alpha = if shown_count > 0 || group.entries.is_empty() {
            master_alpha
        } else {
            0.4 * master_alpha
        };
        let holds_selection = ctrl
            .selected_index
            .map(|idx| group.entries.contains(&idx))
            .unwrap_or(false);
        let is_drop_target = ctrl.drop_target_group == Some(g_idx);

        let bg = if is_drop_target || holds_selection {
            theme.map_card_selected
        } else {
            theme.map_card
        };
        let bcol = if is_drop_target {
            theme.accent
        } else if holds_selection {
            theme.map_card_selected_border
        } else {
            theme.map_card_border
        };
        let bw = if is_drop_target {
            (border_width + 1.5).max(2.2)
        } else if holds_selection {
            (border_width + 1.0).max(1.5)
        } else {
            border_width
        };
        self.fill_rounded_rect(card_rect, corner_radius as f32, bg, card_alpha)?;
        self.draw_rounded_rect(card_rect, corner_radius as f32, bcol, bw as f32, card_alpha)?;

        // Card header
        let padding = 8.0 * m.unit;
        let header_h = 20.0 * m.text_unit;
        let hx = card_x + padding;
        let hy = card_y + padding * 0.7;
        let hw = (card_w - padding * 2.0).max(1.0);

        let lbl_sz = (14.0 * m.text_unit) as f32;
        let lbl_w = self
            .fonts
            .measure_width(&group.label, lbl_sz, true, group.special) as f64;
        self.draw_text(
            &group.label,
            rect_f(hx, hy, lbl_w + 4.0, header_h),
            lbl_sz,
            true,
            group.special,
            if holds_selection {
                theme.accent_text
            } else {
                theme.text
            },
            card_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        let total = group.entries.len();
        let count_str = if total == 0 {
            "empty".to_string()
        } else {
            let cnt = if shown_count < total {
                format!("{shown_count} of {total}")
            } else {
                total.to_string()
            };
            if card_w > 150.0 * m.unit {
                format!("{cnt} {}", if total == 1 { "window" } else { "windows" })
            } else {
                cnt
            }
        };
        let cnt_sz = (11.0 * m.text_unit) as f32;
        let cnt_x = hx + lbl_w + 6.0 * m.unit;
        let cnt_w = self.fonts.measure_width(&count_str, cnt_sz, false, false) as f64;
        self.draw_text(
            &count_str,
            rect_f(cnt_x, hy, cnt_w + 4.0, header_h),
            cnt_sz,
            false,
            false,
            theme.text_soft,
            card_alpha,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;

        // Right side of header: monitor name & "on screen" dot/badge
        let mut rx = hx + hw;
        if group.on_screen && card_w > 190.0 * m.unit {
            let on_sz = (10.0 * m.text_unit) as f32;
            let ow = self.fonts.measure_width("on screen", on_sz, false, false) as f64;
            rx -= ow;
            self.draw_text(
                "on screen",
                rect_f(rx, hy, ow + 2.0, header_h),
                on_sz,
                false,
                false,
                theme.accent_text,
                card_alpha,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
            rx -= 5.0 * m.unit;
        }
        if group.on_screen {
            let dot_w = 6.0 * m.unit;
            rx -= dot_w;
            self.fill_circle(
                rx + dot_w / 2.0,
                hy + header_h / 2.0,
                dot_w / 2.0,
                theme.accent,
                card_alpha,
            )?;
            rx -= 5.0 * m.unit;
        }
        if multi_monitor && !group.monitor.is_empty() && rx > cnt_x + cnt_w + 10.0 {
            let mon_sz = (10.0 * m.text_unit) as f32;
            let mw = self
                .fonts
                .measure_width(&group.monitor, mon_sz, false, false) as f64;
            self.draw_text(
                &group.monitor,
                rect_f((rx - mw).max(cnt_x + cnt_w + 4.0), hy, mw + 2.0, header_h),
                mon_sz,
                false,
                false,
                theme.text_faint,
                card_alpha,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
        }

        // Minimap box
        let box_x = card_x + padding;
        let box_y = hy + header_h + padding * 0.5;
        let box_w = (card_w - padding * 2.0).max(1.0);
        let box_h = (card_y + card_h - box_y - padding).max(1.0);
        let box_rect = rect_f(box_x, box_y, box_w, box_h);

        self.push_rounded_clip(box_rect, 2.0)?;

        let vp = ctrl
            .monitors
            .get(&group.monitor)
            .map(|d| d.rect)
            .unwrap_or(default_vp);
        let win_geoms: Vec<Option<Rect>> = group
            .entries
            .iter()
            .map(|&idx| ctrl.field[idx].geometry)
            .collect();
        let layout_res = layout::minimap_items(Some(vp), &win_geoms, box_w, box_h);

        if let Some(sv) = layout_res.viewport {
            let svr = rect_f(box_x + sv.x, box_y + sv.y, sv.width, sv.height);
            let srad = corner_radius.min(4.0 * m.unit) as f32;
            self.fill_rounded_rect(svr, srad, theme.screen, card_alpha)?;
            self.draw_rounded_rect(
                svr,
                srad,
                if group.on_screen {
                    theme.screen_border_on
                } else {
                    theme.screen_border
                },
                border_width as f32,
                card_alpha,
            )?;
        }

        if group.entries.is_empty() {
            self.draw_text(
                "no windows",
                box_rect,
                (10.0 * m.text_unit) as f32,
                false,
                false,
                theme.text_faint,
                card_alpha,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            )?;
        }

        // Draw minimap tiles (non-selected first, selected on top)
        let mut draw_order: Vec<(usize, usize)> = group
            .entries
            .iter()
            .enumerate()
            .map(|(pos, &entry_idx)| (pos, entry_idx))
            .collect();
        draw_order.sort_by_key(|&(_, entry_idx)| {
            if ctrl.selected_index == Some(entry_idx) {
                2u8
            } else if !ctrl.field[entry_idx].minimized {
                1u8
            } else {
                0u8
            }
        });

        for (pos, entry_idx) in draw_order {
            let entry = &ctrl.field[entry_idx];
            let r = layout_res.rects[pos];
            let inset = (r.width / 6.0).min(1.5);
            let tx = box_x + r.x + inset;
            let ty = box_y + r.y + inset;
            let tw = (r.width - inset * 2.0).max(2.0);
            let th = (r.height - inset * 2.0).max(2.0);

            hits.map_tiles.push((
                entry_idx,
                Rect {
                    x: tx,
                    y: ty,
                    width: tw,
                    height: th,
                },
            ));

            let shown = ctrl.slots.get(entry_idx).copied().unwrap_or(-1) >= 0;
            let selected = ctrl.selected_index == Some(entry_idx);
            let hovered = ctrl.hovered_tile == Some(entry_idx);
            let parked = entry.minimized || layout::outside(win_geoms[pos], Some(vp));
            let tile_alpha = card_alpha
                * if ctrl.dragging_entry == Some(entry_idx) {
                    0.30
                } else if shown {
                    if parked && !selected { 0.62 } else { 1.0 }
                } else {
                    0.22
                };

            let tile_rect = rect_f(tx, ty, tw, th);
            let trad = corner_radius.min(tw / 4.0) as f32;
            let tile_bg = if selected {
                theme.tile_selected
            } else if hovered {
                theme.tile_hover
            } else {
                theme.tile
            };
            self.fill_rounded_rect(tile_rect, trad, tile_bg, tile_alpha)?;

            let cached_snap = self.snapshot_bitmaps.get(&entry.hwnd);
            let shows_frame = cached_snap.is_some() && tw >= 36.0 * m.unit && th >= 24.0 * m.unit;
            let roomy = tw >= 84.0 * m.unit && th >= 46.0 * m.unit;

            if shows_frame {
                if let Some(c) = cached_snap {
                    self.push_rounded_clip(tile_rect, trad)?;
                    let bmp_ref: ID2D1Bitmap = c.bitmap.cast()?;
                    let sz = unsafe { bmp_ref.GetSize() };
                    let src = crop_src_rect(sz.width, sz.height, tw as f32, th as f32);
                    let img_alpha = tile_alpha * if selected || hovered { 1.0 } else { 0.78 };
                    unsafe {
                        self.dc.DrawBitmap(
                            &bmp_ref,
                            Some(&tile_rect),
                            img_alpha,
                            D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                            Some(&src),
                            None,
                        );
                    }
                    unsafe {
                        self.dc.PopLayer();
                    }
                }

                // Bottom strip with small icon and title
                let strip_h = (16.0 * m.text_unit).round().min(th - 2.0);
                let strip_rect = rect_f(
                    tx + 1.0,
                    ty + th - 1.0 - strip_h,
                    (tw - 2.0).max(1.0),
                    strip_h,
                );
                let srad = corner_radius.min(strip_h / 2.0) as f32;
                if roomy {
                    self.fill_rounded_rect(strip_rect, srad, theme.panel, tile_alpha)?;
                }
                let s_icon = 11.0 * m.text_unit;
                self.draw_app_icon(
                    entry,
                    tx + 4.0 * m.unit,
                    strip_rect.top as f64 + (strip_h - s_icon) / 2.0,
                    s_icon,
                    tile_alpha,
                )?;
                if roomy {
                    let title_str = if entry.title.is_empty() {
                        &entry.app_name
                    } else {
                        &entry.title
                    };
                    let tx_lbl = tx + 4.0 * m.unit + s_icon + 4.0 * m.unit;
                    self.draw_text(
                        title_str,
                        rect_f(
                            tx_lbl,
                            strip_rect.top as f64,
                            (tx + tw - 5.0 * m.unit - tx_lbl).max(1.0),
                            strip_h,
                        ),
                        (10.0 * m.text_unit) as f32,
                        false,
                        false,
                        theme.text,
                        tile_alpha,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                    )?;
                }
            } else {
                // Icon alone (and title below if roomy)
                let min_dim = tw.min(th);
                let icon_sz = (min_dim * if roomy { 0.42 } else { 0.64 }).min(26.0 * m.unit);
                if icon_sz >= 7.0 {
                    let offset_y = if roomy { -7.0 * m.text_unit } else { 0.0 };
                    self.draw_app_icon(
                        entry,
                        tx + (tw - icon_sz) / 2.0,
                        ty + (th - icon_sz) / 2.0 + offset_y,
                        icon_sz,
                        tile_alpha,
                    )?;
                }
                if roomy {
                    let title_str = if entry.title.is_empty() {
                        &entry.app_name
                    } else {
                        &entry.title
                    };
                    self.draw_text(
                        title_str,
                        rect_f(
                            tx + 4.0 * m.unit,
                            ty + th - 18.0 * m.text_unit,
                            (tw - 8.0 * m.unit).max(1.0),
                            14.0 * m.text_unit,
                        ),
                        (10.0 * m.text_unit) as f32,
                        false,
                        false,
                        if selected {
                            theme.text
                        } else {
                            theme.text_soft
                        },
                        tile_alpha,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                    )?;
                }
            }

            let out_w = if selected {
                (border_width + 1.0).max(2.0 * m.unit)
            } else {
                border_width
            };
            let out_col = if selected {
                theme.accent
            } else if entry.urgent {
                theme.urgent
            } else if entry.active {
                theme.tile_border_active
            } else {
                theme.tile_border
            };
            self.draw_rounded_rect(tile_rect, trad, out_col, out_w as f32, tile_alpha)?;
        }

        unsafe {
            self.dc.PopLayer();
        }
        Ok(())
    }

    fn draw_app_icon(
        &self,
        entry: &WindowEntry,
        x: f64,
        y: f64,
        size: f64,
        alpha: f32,
    ) -> windows::core::Result<()> {
        if size < 2.0 || alpha <= 0.01 {
            return Ok(());
        }
        if let Some(bmp) = self.icon_bitmaps.get(&entry.hwnd) {
            let bmp_ref: ID2D1Bitmap = bmp.cast()?;
            let dest = rect_f(x, y, size, size);
            unsafe {
                self.dc.DrawBitmap(
                    &bmp_ref,
                    Some(&dest),
                    alpha,
                    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                    None,
                    None,
                );
            }
            return Ok(());
        }

        // Lettered colored rounded tile fallback (exact AppIcon.qml formula)
        let label = if entry.app_name.is_empty() {
            "?"
        } else {
            &entry.app_name
        };
        let mut hash: u32 = 0;
        for b in label.bytes() {
            hash = (hash.wrapping_mul(31) + b as u32) % 3600;
        }
        let hue = hash as f32 / 3600.0;
        let bg = Rgba::hsla(hue, 0.32, 0.42, 1.0);
        let r = rect_f(x, y, size, size);
        let rad = (size * 0.26) as f32;
        self.fill_rounded_rect(r, rad, bg, alpha)?;

        let ch = label
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        self.draw_text(
            &ch,
            r,
            (size * 0.52).max(6.0) as f32,
            true,
            false,
            Rgba::from_u8(244, 244, 244, 255),
            alpha,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_soft_shadow(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        color: Rgba,
        blur: f32,
        spread: f32,
        offset_y: f32,
        alpha: f32,
    ) -> windows::core::Result<()> {
        if alpha <= 0.01 || color.a <= 0.005 {
            return Ok(());
        }
        let steps = 7;
        for i in (1..=steps).rev() {
            let t = i as f32 / steps as f32;
            let expand = spread + blur * 0.55 * t;
            let weight = (1.0 - t * 0.82).powi(2) * (color.a / steps as f32) * 1.9;
            let r = D2D_RECT_F {
                left: rect.left - expand,
                top: rect.top - expand + offset_y,
                right: rect.right + expand,
                bottom: rect.bottom + expand + offset_y,
            };
            self.fill_rounded_rect(r, radius + expand * 0.65, color.with_alpha(weight), alpha)?;
        }
        Ok(())
    }

    fn brush(&self, color: Rgba, alpha: f32) -> windows::core::Result<ID2D1SolidColorBrush> {
        unsafe {
            self.rt
                .CreateSolidColorBrush(&to_d2d_color(color, alpha), None)
        }
    }

    fn fill_rect(&self, rect: D2D_RECT_F, color: Rgba, alpha: f32) -> windows::core::Result<()> {
        let b = self.brush(color, alpha)?;
        unsafe {
            self.rt.FillRectangle(&rect, &b);
        }
        Ok(())
    }

    fn fill_rounded_rect(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        color: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let b = self.brush(color, alpha)?;
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.rt.FillRoundedRectangle(&rr, &b);
        }
        Ok(())
    }

    fn draw_rounded_rect(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        color: Rgba,
        stroke: f32,
        alpha: f32,
    ) -> windows::core::Result<()> {
        if stroke <= 0.0 || alpha <= 0.01 {
            return Ok(());
        }
        let b = self.brush(color, alpha)?;
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.rt.DrawRoundedRectangle(&rr, &b, stroke, None);
        }
        Ok(())
    }

    fn fill_circle(
        &self,
        cx: f64,
        cy: f64,
        r: f64,
        color: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let b = self.brush(color, alpha)?;
        let el = D2D1_ELLIPSE {
            point: D2D_POINT_2F {
                x: cx as f32,
                y: cy as f32,
            },
            radiusX: r as f32,
            radiusY: r as f32,
        };
        unsafe {
            self.rt.FillEllipse(&el, &b);
        }
        Ok(())
    }

    fn draw_circle(
        &self,
        cx: f64,
        cy: f64,
        r: f64,
        color: Rgba,
        stroke: f32,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let b = self.brush(color, alpha)?;
        let el = D2D1_ELLIPSE {
            point: D2D_POINT_2F {
                x: cx as f32,
                y: cy as f32,
            },
            radiusX: r as f32,
            radiusY: r as f32,
        };
        unsafe {
            self.rt.DrawEllipse(&el, &b, stroke, None);
        }
        Ok(())
    }

    fn draw_diamond(
        &self,
        cx: f32,
        cy: f32,
        size: f32,
        radius: f32,
        color: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let mut prev = Default::default();
        unsafe {
            self.rt.GetTransform(&mut prev);
            let rad = std::f32::consts::FRAC_PI_4;
            let cos = rad.cos();
            let sin = rad.sin();
            let mat = windows::Foundation::Numerics::Matrix3x2 {
                M11: cos,
                M12: sin,
                M21: -sin,
                M22: cos,
                M31: cx - cx * cos + cy * sin,
                M32: cy - cx * sin - cy * cos,
            };
            self.rt.SetTransform(&mat);
            let half = size / 2.0;
            let r = D2D_RECT_F {
                left: cx - half,
                top: cy - half,
                right: cx + half,
                bottom: cy + half,
            };
            self.fill_rounded_rect(r, radius, color, alpha)?;
            self.rt.SetTransform(&prev);
        }
        Ok(())
    }

    fn fill_linear_gradient_2(
        &self,
        rect: D2D_RECT_F,
        top: Rgba,
        bottom: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: to_d2d_color(top, alpha),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: to_d2d_color(bottom, alpha),
            },
        ];
        let brush = self.create_linear_brush(rect, &stops)?;
        unsafe {
            self.rt.FillRectangle(&rect, &brush);
        }
        Ok(())
    }

    fn fill_linear_gradient_3(
        &self,
        rect: D2D_RECT_F,
        top: Rgba,
        mid: Rgba,
        mid_pos: f32,
        bottom: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: to_d2d_color(top, alpha),
            },
            D2D1_GRADIENT_STOP {
                position: mid_pos,
                color: to_d2d_color(mid, alpha),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: to_d2d_color(bottom, alpha),
            },
        ];
        let brush = self.create_linear_brush(rect, &stops)?;
        unsafe {
            self.rt.FillRectangle(&rect, &brush);
        }
        Ok(())
    }

    fn fill_rounded_gradient_2(
        &self,
        rect: D2D_RECT_F,
        radius: f32,
        top: Rgba,
        bottom: Rgba,
        alpha: f32,
    ) -> windows::core::Result<()> {
        let stops = [
            D2D1_GRADIENT_STOP {
                position: 0.0,
                color: to_d2d_color(top, alpha),
            },
            D2D1_GRADIENT_STOP {
                position: 1.0,
                color: to_d2d_color(bottom, alpha),
            },
        ];
        let brush = self.create_linear_brush(rect, &stops)?;
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.rt.FillRoundedRectangle(&rr, &brush);
        }
        Ok(())
    }

    fn create_linear_brush(
        &self,
        rect: D2D_RECT_F,
        stops: &[D2D1_GRADIENT_STOP],
    ) -> windows::core::Result<ID2D1LinearGradientBrush> {
        unsafe {
            let col = self.rt.CreateGradientStopCollection(
                stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: D2D_POINT_2F {
                    x: rect.left,
                    y: rect.top,
                },
                endPoint: D2D_POINT_2F {
                    x: rect.left,
                    y: rect.bottom,
                },
            };
            let bp = D2D1_BRUSH_PROPERTIES {
                opacity: 1.0,
                transform: windows::Foundation::Numerics::Matrix3x2::identity(),
            };
            self.rt.CreateLinearGradientBrush(&props, Some(&bp), &col)
        }
    }

    fn push_rounded_clip(&self, rect: D2D_RECT_F, radius: f32) -> windows::core::Result<()> {
        let rr = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            let geom: ID2D1RoundedRectangleGeometry =
                self.d2d_factory.CreateRoundedRectangleGeometry(&rr)?;
            let mut params = D2D1_LAYER_PARAMETERS {
                contentBounds: rect,
                geometricMask: std::mem::ManuallyDrop::new(Some(geom.cast()?)),
                maskAntialiasMode: Default::default(),
                maskTransform: windows::Foundation::Numerics::Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, None);
            let _ = std::mem::ManuallyDrop::take(&mut params.geometricMask);
            let _ = std::mem::ManuallyDrop::take(&mut params.opacityBrush);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        text: &str,
        rect: D2D_RECT_F,
        size: f32,
        bold: bool,
        italic: bool,
        color: Rgba,
        alpha: f32,
        h_align: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
        v_align: windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT,
    ) -> windows::core::Result<()> {
        if alpha <= 0.01 || text.is_empty() {
            return Ok(());
        }
        let Some(fmt) = self.fonts.text_format(size, bold, italic) else {
            return Ok(());
        };
        let max_w = (rect.right - rect.left).max(1.0);
        let max_h = (rect.bottom - rect.top).max(1.0);
        let Some(layout) = self.fonts.create_layout(text, &fmt, max_w, max_h) else {
            return Ok(());
        };
        let b = self.brush(color, alpha)?;
        unsafe {
            let _ = layout.SetTextAlignment(h_align);
            let _ = layout.SetParagraphAlignment(v_align);
            self.rt.DrawTextLayout(
                D2D_POINT_2F {
                    x: rect.left,
                    y: rect.top,
                },
                &layout,
                &b,
                Default::default(),
            );
        }
        Ok(())
    }
}

fn to_d2d_color(c: Rgba, alpha_mul: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: (c.a * alpha_mul).clamp(0.0, 1.0),
    }
}

fn rect_f(x: f64, y: f64, w: f64, h: f64) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x as f32,
        top: y as f32,
        right: (x + w.max(0.0)) as f32,
        bottom: (y + h.max(0.0)) as f32,
    }
}

fn crop_src_rect(src_w: f32, src_h: f32, dst_w: f32, dst_h: f32) -> D2D_RECT_F {
    if src_w <= 0.0 || src_h <= 0.0 || dst_w <= 0.0 || dst_h <= 0.0 {
        return D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: src_w,
            bottom: src_h,
        };
    }
    let src_aspect = src_w / src_h;
    let dst_aspect = dst_w / dst_h;
    if src_aspect > dst_aspect {
        let crop_w = src_h * dst_aspect;
        let left = (src_w - crop_w) / 2.0;
        D2D_RECT_F {
            left,
            top: 0.0,
            right: left + crop_w,
            bottom: src_h,
        }
    } else {
        let crop_h = src_w / dst_aspect;
        let top = (src_h - crop_h) / 2.0;
        D2D_RECT_F {
            left: 0.0,
            top,
            right: src_w,
            bottom: top + crop_h,
        }
    }
}

// Keep unused imports silenced cleanly if needed across feature gates
#[allow(dead_code)]
fn _unused_markers() {
    let _ = (
        D2D1_BEZIER_SEGMENT {
            point1: D2D_POINT_2F::default(),
            point2: D2D_POINT_2F::default(),
            point3: D2D_POINT_2F::default(),
        },
        D2D1_BITMAP_PROPERTIES::default(),
        D2D1_FIGURE_BEGIN_FILLED,
        D2D1_FIGURE_END_CLOSED,
        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
        SW_SHOW,
        RECT::default(),
        STGM(0),
    );
}
