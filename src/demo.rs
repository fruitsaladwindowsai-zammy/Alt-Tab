//! Synthetic 12-window / 8-workspace field and colorful mock previews for `--demo` and `--render-demo`.

use crate::controller::{Controller, MonitorDesc, SwitchMode};
use crate::field::WorkspaceDescriptor;
use crate::layout::Rect;
use crate::recency::WindowCandidate;
use crate::win_capture::{BgraImage, MediaStore, SnapshotEntry};
use std::sync::Arc;

pub fn setup_demo(ctrl: &mut Controller, store: &mut MediaStore, width: f64, height: f64, now_ms: u64) {
    let vp = Rect { x: 0.0, y: 0.0, width, height };
    let monitors = vec![MonitorDesc {
        name: "DISPLAY1".into(),
        rect: vp,
        focused: true,
    }];

    let workspaces = vec![
        WorkspaceDescriptor { key: "id:1".into(), id: Some(1), name: "1".into(), monitor: "DISPLAY1".into(), on_screen: true },
        WorkspaceDescriptor { key: "id:2".into(), id: Some(2), name: "2".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "id:3".into(), id: Some(3), name: "3".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "id:4".into(), id: Some(4), name: "4".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "id:5".into(), id: Some(5), name: "5".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "id:6".into(), id: Some(6), name: "6".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "id:7".into(), id: Some(7), name: "7".into(), monitor: "DISPLAY1".into(), on_screen: false },
        WorkspaceDescriptor { key: "special:scratchpad".into(), id: None, name: "special:scratchpad".into(), monitor: "DISPLAY1".into(), on_screen: false },
    ];

    let specs: &[(&str, &str, &str, Option<i64>, &str, u64, Rect, [u8; 3], u8)] = &[
        ("Palette.js — fathom", "Code", "id:1", Some(1), "1", 0, Rect { x: 24.0, y: 24.0, width: 1120.0, height: 1032.0 }, [122, 162, 247], 0),
        ("Omarchy — The Manual", "Browser", "id:2", Some(2), "2", 45, Rect { x: 24.0, y: 24.0, width: 1180.0, height: 1032.0 }, [138, 132, 247], 1),
        ("~/Projects/fathom", "Terminal", "id:1", Some(1), "1", 2, Rect { x: 1164.0, y: 24.0, width: 732.0, height: 1032.0 }, [125, 207, 255], 2),
        ("Now Playing — Low Tide", "Music", "id:3", Some(3), "3", 6, Rect { x: 120.0, y: 80.0, width: 1680.0, height: 920.0 }, [224, 118, 92], 3),
        ("#design — 3 new", "Chat", "id:4", Some(4), "4", 12, Rect { x: 24.0, y: 24.0, width: 1260.0, height: 1032.0 }, [187, 154, 247], 4),
        ("Release notes", "Notes", "id:4", Some(4), "4", 19, Rect { x: 1304.0, y: 24.0, width: 592.0, height: 1032.0 }, [158, 206, 106], 5),
        ("btop", "System Monitor", "special:scratchpad", None, "special:scratchpad", 33, Rect { x: 180.0, y: 120.0, width: 1560.0, height: 840.0 }, [247, 118, 142], 6),
        ("Iceland, 2026", "Photos", "id:5", Some(5), "5", 220, Rect { x: 24.0, y: 24.0, width: 1220.0, height: 1032.0 }, [115, 218, 202], 7),
        ("Starfall", "Wallpaper", "id:6", Some(6), "6", 720, Rect { x: 24.0, y: 24.0, width: 1872.0, height: 1032.0 }, [180, 120, 220], 8),
        ("September", "Calendar", "id:7", Some(7), "7", 2600, Rect { x: 24.0, y: 24.0, width: 1872.0, height: 1032.0 }, [122, 162, 247], 9),
        ("Inspector", "DevTools", "id:2", Some(2), "2", 4800, Rect { x: 1224.0, y: 24.0, width: 672.0, height: 1032.0 }, [224, 175, 104], 10),
        ("Files — Projects", "Files", "id:5", Some(5), "5", 9200, Rect { x: 1264.0, y: 24.0, width: 632.0, height: 1032.0 }, [125, 207, 255], 11),
    ];

    ctrl.recency = crate::recency::RecencyState::new();
    let mut cands = Vec::new();
    for (i, &(title, app, ws_key, ws_id, ws_name, secs_ago, geom, accent, kind)) in specs.iter().enumerate() {
        let hwnd = 1000 + i as isize;
        if i == 0 {
            ctrl.recency.active = Some(hwnd);
            ctrl.recency.last_active.insert(hwnd, now_ms);
        } else {
            ctrl.recency.last_active.insert(hwnd, now_ms.saturating_sub(secs_ago * 1000));
        }

        let snap = synthetic_window_preview(640, 380, accent, kind);
        store.snapshots.insert(
            hwnd,
            SnapshotEntry {
                image: snap,
                taken_at_ms: now_ms.saturating_sub(5000),
                generation: (i + 1) as u64,
            },
        );

        cands.push(WindowCandidate {
            hwnd,
            pid: 5000 + i as u32,
            title: title.into(),
            app_id: app.to_ascii_lowercase(),
            app_name: app.into(),
            exe_path: String::new(),
            workspace_key: ws_key.into(),
            workspace_id: ws_id,
            workspace_name: ws_name.into(),
            monitor_name: "DISPLAY1".into(),
            geometry: Some(geom),
            minimized: false,
            on_current_desktop: ws_id == Some(1),
            urgent: false,
            z_rank: i,
        });
    }

    ctrl.demo_mode = true;
    ctrl.open_field(SwitchMode::Hold, 1, &cands, &workspaces, monitors, vp, Some(1000), now_ms);
    ctrl.reveal_now(now_ms as f64);
    ctrl.reveal_opacity = crate::controller::AnimValue::snap(1.0);
    ctrl.camera_slot = crate::controller::AnimValue::snap(1.0);
    ctrl.scene_depth = crate::controller::AnimValue::snap(ctrl.selected_entry().map(|e| e.depth).unwrap_or(0.0));
}

fn synthetic_window_preview(w: u32, h: u32, accent: [u8; 3], kind: u8) -> BgraImage {
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let bg = [26u8, 29, 41];
    let chrome = [34u8, 38, 54];
    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let mut rgb = if y < 28 { chrome } else { bg };
            if kind == 1 {
                // Hero card + 3 feature cards (matching Fathom screenshot)
                if x > 70 && x < w - 70 && y > 48 && y < 185 {
                    let t = (x - 70) as f32 / (w - 140) as f32;
                    rgb = [
                        (182.0 * (1.0 - t) + 120.0 * t) as u8,
                        (142.0 * (1.0 - t) + 178.0 * t) as u8,
                        252,
                    ];
                } else if y > 205 && y < 300 {
                    let bw = (w - 170) / 3;
                    if x > 70 && x < 70 + bw {
                        rgb = [148, 198, 102];
                    } else if x > 85 + bw && x < 85 + bw * 2 {
                        rgb = [122, 196, 232];
                    } else if x > 100 + bw * 2 && x < 100 + bw * 3 {
                        rgb = [232, 152, 92];
                    }
                }
            } else if kind == 3 {
                // Album art + track rows
                let dx = x as i32 - 140;
                let dy = y as i32 - 190;
                if dx * dx + dy * dy < 85 * 85 {
                    rgb = [224, 118, 92];
                } else if x > 280 && x < w - 60 && y > 80 && y < h - 60 && (y % 26) < 10 {
                    rgb = [76, 84, 110];
                }
            } else if kind == 6 {
                // btop spectrum bars
                if y > 70 && y < 210 && x > 40 && x < w - 40 && (x % 14) < 10 {
                    let bar_h = ((x as f32 * 0.08).sin().abs() * 120.0) as u32;
                    if 210 - y < bar_h {
                        rgb = if (x / 14) % 2 == 0 { [158, 206, 106] } else { [247, 118, 142] };
                    }
                }
            } else {
                // Code/terminal lines
                if x > 42 && x < w - 80 && y > 54 && y < h - 40 && (y % 22) < 9 {
                    let line_len = 120 + (((y / 22) * 73) % (w - 240));
                    if x < 42 + line_len {
                        rgb = if (y / 22) % 4 == 0 {
                            accent
                        } else {
                            [92, 102, 134]
                        };
                    }
                }
            }
            buf[idx] = rgb[2];
            buf[idx + 1] = rgb[1];
            buf[idx + 2] = rgb[0];
            buf[idx + 3] = 255;
        }
    }
    BgraImage {
        width: w,
        height: h,
        bgra_premul: Arc::new(buf),
    }
}
