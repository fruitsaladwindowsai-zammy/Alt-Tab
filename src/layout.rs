//! Geometry for the Deep stack, the Sounding Line, and the Workspace Map.

use crate::depth;

pub const SIZE_Q: f64 = 0.82;
pub const POS_Q: f64 = 0.86;
pub const RISE: f64 = 0.10;
pub const PASSED_FADE: f64 = 0.60;
pub const PASSED_GROWTH: f64 = 0.20;
pub const VISIBLE_STEPS: f64 = 5.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeepStage {
    pub front_x: f64,
    pub front_y: f64,
    pub front_width: f64,
    pub front_height: f64,
    pub step_x: f64,
    pub step_y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlaneGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub opacity: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundingMark {
    pub y: f64,
    pub column: usize,
    pub hidden: bool,
    pub more: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MinimapLayout {
    pub rects: Vec<Rect>,
    pub viewport: Option<Rect>,
    pub bounds: Option<Rect>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapCardMetrics {
    pub widths: Vec<f64>,
    pub height: f64,
    pub scale: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct OverlayMetrics {
    pub width: f64,
    pub height: f64,
    pub unit: f64,
    pub text_unit: f64,
    pub margin: f64,
    pub map_height: f64,
    pub caption_height: f64,
    pub deep_top: f64,
    pub gauge_width: f64,
    pub stage: DeepStage,
}

pub fn overlay_metrics(width: f64, height: f64, font_scale: f64) -> OverlayMetrics {
    let unit = (height / 1000.0).clamp(0.75, 1.6);
    let text_unit = unit * font_scale.clamp(0.85, 1.6);
    let margin = (width * 0.022).max(16.0);
    let map_height = (height * 0.2).min(210.0 * unit).max(96.0);
    let caption_height = 62.0 * unit;
    let deep_top = margin + 44.0 * unit;
    let gauge_width = if width >= 1000.0 { 112.0 * text_unit } else { 0.0 };
    let aspect = width / height.max(1.0);
    let stage = deep_stage(
        width,
        height,
        deep_top,
        map_height + caption_height + margin + 30.0 * unit,
        aspect,
        margin + gauge_width,
    );
    OverlayMetrics {
        width,
        height,
        unit,
        text_unit,
        margin,
        map_height,
        caption_height,
        deep_top,
        gauge_width,
        stage,
    }
}

pub fn step_sum(r: f64) -> f64 {
    if r <= 0.0 {
        0.0
    } else {
        (1.0 - POS_Q.powf(r)) / (1.0 - POS_Q)
    }
}

pub fn fit(box_width: f64, box_height: f64, aspect: f64) -> Rect {
    let a = if aspect > 0.0 && aspect.is_finite() { aspect } else { 1.6 };
    let mut width = box_width;
    let mut height = width / a;
    if height > box_height {
        height = box_height;
        width = height * a;
    }
    Rect {
        x: (box_width - width) / 2.0,
        y: (box_height - height) / 2.0,
        width: width.max(1.0),
        height: height.max(1.0),
    }
}

pub fn deep_stage(
    width: f64,
    height: f64,
    top: f64,
    bottom: f64,
    aspect: f64,
    left: f64,
) -> DeepStage {
    let inset = left.max(0.0);
    let a = if aspect.is_finite() { aspect.clamp(1.2, 2.4) } else { 1.6 };
    let available = (height - top - bottom).max(1.0);
    let reach = step_sum(VISIBLE_STEPS);
    let mut front_height = available / (1.0 + RISE * reach);
    let mut front_width = front_height * a;
    if front_width > width * 0.5 {
        front_width = width * 0.5;
        front_height = front_width / a;
    }
    let room = width - inset;
    if front_width > room * 0.55 {
        front_width = room * 0.55;
        front_height = front_width / a;
    }
    let margin = width * 0.03;
    let spread = (room - margin * 2.0 - front_width).min(front_width * 0.85).max(0.0);
    let stack_height = front_height * (1.0 + RISE * reach);
    let stack_top = top + (available - stack_height) / 2.0;

    DeepStage {
        front_x: inset + (room - front_width - spread) / 2.0 + front_width / 2.0,
        front_y: stack_top + RISE * front_height * reach + front_height / 2.0,
        front_width,
        front_height,
        step_x: if reach > 0.0 { spread / reach } else { 0.0 },
        step_y: RISE * front_height,
    }
}

pub fn deep_plane(stage: DeepStage, r: f64) -> Option<PlaneGeometry> {
    if !(-1.5..=VISIBLE_STEPS + 1.0).contains(&r) {
        return None;
    }
    if r < 0.0 {
        let passed = (-r).min(1.5);
        let grow = 1.0 + PASSED_GROWTH * passed.min(1.0);
        return Some(PlaneGeometry {
            x: stage.front_x - passed * stage.front_width * 0.5,
            y: stage.front_y + passed * stage.front_height * 0.12,
            width: stage.front_width * grow,
            height: stage.front_height * grow,
            scale: grow,
            opacity: (1.0 - passed / PASSED_FADE).clamp(0.0, 1.0),
            z: 1000.0 + passed,
        });
    }
    let scale = SIZE_Q.powf(r);
    let sum = step_sum(r);
    let width = stage.front_width * scale;
    let height = stage.front_height * scale;
    let right = stage.front_x + stage.front_width / 2.0 + stage.step_x * sum;
    let top = stage.front_y - stage.front_height / 2.0 - stage.step_y * sum;
    Some(PlaneGeometry {
        x: right - width / 2.0,
        y: top + height / 2.0,
        width,
        height,
        scale,
        opacity: (VISIBLE_STEPS + 1.0 - r).clamp(0.0, 1.0),
        z: 1000.0 - r,
    })
}

pub fn sounding_y(depth: f64, top: f64, bottom: f64) -> f64 {
    top + (bottom - top) * depth::clamp_depth(depth) / depth::MAX_DEPTH
}

pub fn sounding_marks(
    depths: &[f64],
    top: f64,
    bottom: f64,
    spacing: f64,
    max_columns: usize,
) -> Vec<SoundingMark> {
    let gap = if spacing > 0.0 { spacing } else { 6.0 };
    let limit = if max_columns > 0 { max_columns } else { usize::MAX };

    struct Row {
        y: f64,
        count: usize,
        last_visible_idx: Option<usize>,
    }

    let mut marks: Vec<SoundingMark> = Vec::with_capacity(depths.len());
    let mut rows: Vec<Row> = Vec::new();

    for &d in depths {
        let y = sounding_y(d, top, bottom);
        let row_idx = rows.iter().position(|r| (r.y - y).abs() < gap);
        let row = match row_idx {
            Some(idx) => &mut rows[idx],
            None => {
                rows.push(Row { y, count: 0, last_visible_idx: None });
                rows.last_mut().unwrap()
            }
        };
        let column = row.count;
        row.count += 1;
        let hidden = column >= limit;
        if hidden {
            if let Some(last) = row.last_visible_idx {
                marks[last].more += 1;
            }
        } else {
            row.last_visible_idx = Some(marks.len());
        }
        marks.push(SoundingMark {
            y: row.y,
            column,
            hidden,
            more: 0,
        });
    }
    marks
}

pub fn nearest_mark(marks: &[SoundingMark], y: f64, column: usize) -> Option<usize> {
    let mut best = None;
    let mut best_score = f64::INFINITY;
    for (i, mark) in marks.iter().enumerate() {
        if mark.hidden {
            continue;
        }
        let col_diff = (mark.column as f64 - column as f64).abs();
        let score = (mark.y - y).abs() * 4.0 + col_diff;
        if score < best_score {
            best_score = score;
            best = Some(i);
        }
    }
    best
}

pub fn minimap_bounds(viewport: Rect, windows: &[Option<Rect>]) -> Rect {
    let mut left = viewport.x;
    let mut top = viewport.y;
    let mut right = viewport.x + viewport.width;
    let mut bottom = viewport.y + viewport.height;
    for w in windows.iter().flatten() {
        if w.width > 0.0 && w.height > 0.0 && w.x.is_finite() && w.y.is_finite() {
            left = left.min(w.x);
            top = top.min(w.y);
            right = right.max(w.x + w.width);
            bottom = bottom.max(w.y + w.height);
        }
    }
    Rect {
        x: left,
        y: top,
        width: (right - left).max(1.0),
        height: (bottom - top).max(1.0),
    }
}

pub fn outside(rect: Option<Rect>, area: Option<Rect>) -> bool {
    let (Some(r), Some(a)) = (rect, area) else {
        return false;
    };
    r.x >= a.x + a.width
        || r.x + r.width <= a.x
        || r.y >= a.y + a.height
        || r.y + r.height <= a.y
}

pub fn map_rect(rect: Rect, bounds: Rect, box_width: f64, box_height: f64) -> Rect {
    let scale = (box_width / bounds.width).min(box_height / bounds.height);
    let offset_x = (box_width - bounds.width * scale) / 2.0;
    let offset_y = (box_height - bounds.height * scale) / 2.0;
    Rect {
        x: offset_x + (rect.x - bounds.x) * scale,
        y: offset_y + (rect.y - bounds.y) * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

pub fn grid_rect(position: usize, count: usize, box_width: f64, box_height: f64, gap: f64) -> Rect {
    let n = count.max(1);
    let columns = ((n as f64 * box_width / box_height.max(1.0)).sqrt().ceil() as usize).clamp(1, n);
    let rows = (n + columns - 1) / columns;
    let g = if gap > 0.0 { gap } else { 2.0 };
    let width = (box_width - g * (columns as f64 + 1.0)) / columns as f64;
    let height = (box_height - g * (rows as f64 + 1.0)) / rows as f64;
    let column = position % columns;
    let row = position / columns;
    Rect {
        x: g + column as f64 * (width + g),
        y: g + row as f64 * (height + g),
        width: width.max(1.0),
        height: height.max(1.0),
    }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() <= 28.0
        && (a.y - b.y).abs() <= 28.0
        && (a.width - b.width).abs() <= 28.0
        && (a.height - b.height).abs() <= 28.0
}

fn split_shared_rects(rects: &mut [Rect], known: &[Option<Rect>]) {
    let mut done = vec![false; rects.len()];
    for a in 0..rects.len() {
        if done[a] {
            continue;
        }
        let Some(ka) = known[a] else { continue };
        let mut same = vec![a];
        for b in (a + 1)..rects.len() {
            if !done[b] {
                if let Some(kb) = known[b] {
                    if same_rect(ka, kb) {
                        same.push(b);
                    }
                }
            }
        }
        let base = rects[a];
        let part = base.width / same.len() as f64;
        for (k, &idx) in same.iter().enumerate() {
            rects[idx] = Rect {
                x: base.x + k as f64 * part,
                y: base.y,
                width: part,
                height: base.height,
            };
            done[idx] = true;
        }
    }
}

pub fn minimap_items(
    viewport: Option<Rect>,
    windows: &[Option<Rect>],
    box_width: f64,
    box_height: f64,
) -> MinimapLayout {
    let mut known = Vec::with_capacity(windows.len());
    let mut any_known = false;
    for w in windows {
        match w {
            Some(r) if r.width > 0.0 && r.height > 0.0 && r.x.is_finite() && r.y.is_finite() => {
                known.push(Some(*r));
                any_known = true;
            }
            _ => {
                known.push(None);
            }
        }
    }

    let vp = match viewport {
        Some(v) if v.width > 0.0 && v.height > 0.0 => v,
        _ => {
            let rects = (0..windows.len())
                .map(|g| grid_rect(g, windows.len(), box_width, box_height, 3.0))
                .collect();
            return MinimapLayout { rects, viewport: None, bounds: None };
        }
    };

    if !any_known && !windows.is_empty() {
        let rects = (0..windows.len())
            .map(|g| grid_rect(g, windows.len(), box_width, box_height, 3.0))
            .collect();
        return MinimapLayout {
            rects,
            viewport: Some(map_rect(vp, vp, box_width, box_height)),
            bounds: Some(vp),
        };
    }

    let bounds = minimap_bounds(vp, &known);
    let mapped_vp = map_rect(vp, bounds, box_width, box_height);
    let missing_count = known.iter().filter(|k| k.is_none()).count();
    let mut missing_idx = 0usize;
    let mut rects = Vec::with_capacity(known.len());
    for k in &known {
        if let Some(r) = k {
            rects.push(map_rect(*r, bounds, box_width, box_height));
        } else {
            let slot = grid_rect(missing_idx, missing_count.max(1), mapped_vp.width, mapped_vp.height, 3.0);
            rects.push(Rect {
                x: mapped_vp.x + slot.x,
                y: mapped_vp.y + slot.y,
                width: slot.width,
                height: slot.height,
            });
            missing_idx += 1;
        }
    }
    split_shared_rects(&mut rects, &known);
    MinimapLayout {
        rects,
        viewport: Some(mapped_vp),
        bounds: Some(bounds),
    }
}

pub fn map_cards(
    aspects: &[f64],
    available: f64,
    height: f64,
    gap: f64,
    chrome: f64,
    min_width: f64,
) -> MapCardMetrics {
    let mut widths = Vec::with_capacity(aspects.len());
    let mut total = 0.0;
    for &raw_a in aspects {
        let aspect = if raw_a.is_finite() { raw_a.clamp(0.8, 5.0) } else { 1.6 };
        let w = height * aspect + chrome;
        widths.push(w);
        total += w;
    }
    let gap_total = gap * aspects.len().saturating_sub(1) as f64;
    total += gap_total;

    let scale = if total > available && total > gap_total {
        (available - gap_total) / (total - gap_total)
    } else {
        1.0
    };

    let mut used = 0.0;
    for w in &mut widths {
        *w = (*w * scale).max(min_width);
        used += *w;
    }
    let room = (available - gap_total).max(0.0);
    if used > room && used > 0.0 {
        for w in &mut widths {
            *w *= room / used;
        }
    }

    MapCardMetrics {
        widths,
        height: height * scale.clamp(0.55, 1.0),
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_plane_fades_at_band_edges() {
        let stage = deep_stage(1920.0, 1080.0, 60.0, 260.0, 16.0 / 9.0, 140.0);
        let front = deep_plane(stage, 0.0).unwrap();
        assert!((front.opacity - 1.0).abs() < 1e-6);
        assert_eq!(front.z, 1000.0);
        let far = deep_plane(stage, 6.0).unwrap();
        assert_eq!(far.opacity, 0.0);
        let passed = deep_plane(stage, -0.6).unwrap();
        assert!(passed.opacity.abs() < 1e-6);
        assert!(deep_plane(stage, 6.1).is_none());
    }

    #[test]
    fn sounding_marks_group_close_depths() {
        let depths = vec![0.0, 0.01, 0.02, 4.0];
        let marks = sounding_marks(&depths, 0.0, 400.0, 8.0, 2);
        assert_eq!(marks[0].column, 0);
        assert_eq!(marks[1].column, 1);
        assert!(marks[2].hidden);
        assert_eq!(marks[1].more, 1);
        assert_eq!(marks[3].column, 0);
    }

    #[test]
    fn minimap_keeps_viewport_and_splits_maximized_windows() {
        let vp = Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 };
        let w1 = Some(Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1040.0 });
        let w2 = Some(Rect { x: -7.0, y: 0.0, width: 1934.0, height: 1047.0 });
        let layout = minimap_items(Some(vp), &[w1, w2, None], 200.0, 120.0);
        assert!(layout.viewport.is_some());
        assert_eq!(layout.rects.len(), 3);
        // The two nearly-maximized windows should split horizontally rather than completely overlapping.
        assert!((layout.rects[0].x - layout.rects[1].x).abs() > 20.0);
    }
}
