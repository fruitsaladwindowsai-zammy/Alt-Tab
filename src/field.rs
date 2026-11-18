//! Filtering, age/fathom formatting, workspace grouping, and selection navigation.

use crate::layout::Rect;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub index: usize,
    pub hwnd: isize,
    #[allow(dead_code)]
    pub pid: u32,
    pub title: String,
    pub app_id: String,
    pub app_name: String,
    pub exe_path: String,
    pub workspace_key: String,
    pub workspace_id: Option<i64>,
    pub workspace_name: String,
    pub monitor_name: String,
    pub geometry: Option<Rect>,
    pub minimized: bool,
    pub on_current_desktop: bool,
    pub urgent: bool,
    pub active: bool,
    pub estimated: bool,
    pub seconds: f64,
    pub depth: f64,
    pub z_rank: usize,
}

#[derive(Clone, Debug)]
pub struct WorkspaceDescriptor {
    pub key: String,
    pub id: Option<i64>,
    pub name: String,
    pub monitor: String,
    pub on_screen: bool,
}

#[derive(Clone, Debug)]
pub struct WorkspaceGroup {
    pub key: String,
    pub id: Option<i64>,
    pub name: String,
    pub label: String,
    pub special: bool,
    pub monitor: String,
    pub on_screen: bool,
    pub entries: Vec<usize>,
}

pub fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn clean_app_name(raw_id: &str, friendly_name: &str) -> String {
    let named = friendly_name.trim();
    if !named.is_empty() {
        return named.to_string();
    }
    let id = raw_id.trim();
    if id.is_empty() {
        return "App".to_string();
    }
    let stem = id
        .strip_suffix(".exe")
        .or_else(|| id.strip_suffix(".EXE"))
        .unwrap_or(id);
    let last = if stem.contains('.') && stem.split('.').count() > 2 {
        stem.split('.').next_back().unwrap_or(stem)
    } else {
        stem
    };
    last.split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn age_label(seconds: f64, active: bool, estimated: bool) -> String {
    if active {
        return "focused".to_string();
    }
    if estimated {
        return "earlier".to_string();
    }
    if !seconds.is_finite() {
        return "not used yet".to_string();
    }
    if seconds < 10.0 {
        return "just now".to_string();
    }
    if seconds < 60.0 {
        return format!("{} s ago", seconds.floor() as u64);
    }
    if seconds < 3600.0 {
        return format!("{} min ago", (seconds / 60.0).floor() as u64);
    }
    if seconds < 86400.0 {
        return format!("{} h ago", (seconds / 3600.0).floor() as u64);
    }
    format!("{} d ago", (seconds / 86400.0).floor() as u64)
}

pub fn age_short(seconds: f64, active: bool, estimated: bool) -> String {
    if active {
        return "now".to_string();
    }
    if estimated || !seconds.is_finite() {
        return String::new();
    }
    if seconds < 10.0 {
        return "now".to_string();
    }
    if seconds < 60.0 {
        return format!("{}s", seconds.floor() as u64);
    }
    if seconds < 3600.0 {
        return format!("{}m", (seconds / 60.0).floor() as u64);
    }
    if seconds < 86400.0 {
        return format!("{}h", (seconds / 3600.0).floor() as u64);
    }
    format!("{}d", (seconds / 86400.0).floor() as u64)
}

pub fn depth_mark_label(depth: usize) -> String {
    if depth == 0 {
        return "now".to_string();
    }
    if depth >= 8 {
        return "2h+".to_string();
    }
    let seconds = 30.0 * (2.0_f64.powi(depth as i32) - 1.0);
    age_short(seconds, false, false)
}

pub fn fathom_label(depth: f64, active: bool, estimated: bool) -> String {
    if active || estimated {
        return String::new();
    }
    let d = (depth.max(0.0) * 10.0).round() / 10.0;
    if d == 0.0 {
        return "at the surface".to_string();
    }
    if (d - 1.0).abs() < 1e-6 {
        return "1 fathom".to_string();
    }
    if (d - d.round()).abs() < 1e-6 {
        return format!("{} fathoms", d.round() as i64);
    }
    format!("{:.1} fathoms", d)
}

pub fn is_special_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("special") || lower == "scratchpad"
}

pub fn workspace_label(name: &str, id: Option<i64>) -> String {
    let trimmed = name.trim();
    if is_special_name(trimmed) {
        let rest = trimmed
            .strip_prefix("special:")
            .or_else(|| trimmed.strip_prefix("special"))
            .unwrap_or(trimmed);
        return if rest.is_empty() {
            "scratchpad".to_string()
        } else {
            rest.to_string()
        };
    }
    if !trimmed.is_empty() {
        return trimmed.strip_prefix("name:").unwrap_or(trimmed).to_string();
    }
    id.map(|n| n.to_string()).unwrap_or_else(|| "1".to_string())
}

pub fn normalize_query(query: &str) -> String {
    query
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn matches_query(entry: &WindowEntry, query: &str) -> bool {
    let norm = normalize_query(query);
    if norm.is_empty() {
        return true;
    }
    let ws = workspace_label(&entry.workspace_name, entry.workspace_id);
    let haystack =
        format!("{} {} {} {}", entry.app_name, entry.app_id, entry.title, ws).to_ascii_lowercase();
    norm.split(' ').all(|tok| haystack.contains(tok))
}

pub fn visible_order(
    entries: &[WindowEntry],
    closed: &HashSet<isize>,
    query: &str,
    show_scratchpads: bool,
) -> Vec<usize> {
    let mut order = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if closed.contains(&entry.hwnd) {
            continue;
        }
        if !show_scratchpads && is_special_name(&entry.workspace_name) {
            continue;
        }
        if !matches_query(entry, query) {
            continue;
        }
        order.push(i);
    }
    order
}

pub fn slots_for(order: &[usize], count: usize) -> Vec<isize> {
    let mut slots = vec![-1; count];
    for (slot, &entry_idx) in order.iter().enumerate() {
        if entry_idx < count {
            slots[entry_idx] = slot as isize;
        }
    }
    slots
}

pub fn slot_of(order: &[usize], index: Option<usize>) -> Option<usize> {
    let idx = index?;
    order.iter().position(|&item| item == idx)
}

pub fn step(order: &[usize], current: Option<usize>, delta: isize, wrap: bool) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let Some(slot) = slot_of(order, current) else {
        return Some(order[0]);
    };
    let count = order.len() as isize;
    let mut next = slot as isize + delta;
    if wrap {
        next = ((next % count) + count) % count;
    } else {
        next = next.clamp(0, count - 1);
    }
    Some(order[next as usize])
}

pub fn reselect(
    order: &[usize],
    current: Option<usize>,
    previous_order: &[usize],
) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    if let Some(idx) = current {
        if order.contains(&idx) {
            return Some(idx);
        }
        if let Some(at) = previous_order.iter().position(|&x| x == idx) {
            for &candidate in &previous_order[at + 1..] {
                if order.contains(&candidate) {
                    return Some(candidate);
                }
            }
            for &candidate in previous_order[..at].iter().rev() {
                if order.contains(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    Some(order[0])
}

fn workspace_rank(group: &WorkspaceGroup) -> u8 {
    if group.special {
        2
    } else if group.id.map(|id| id > 0).unwrap_or(false) && !group.name.starts_with("name:") {
        0
    } else {
        1
    }
}

pub fn workspace_groups(
    entries: &[WindowEntry],
    extra: &[WorkspaceDescriptor],
) -> Vec<WorkspaceGroup> {
    let mut groups: Vec<WorkspaceGroup> = Vec::new();

    let ensure_group = |groups: &mut Vec<WorkspaceGroup>,
                        key: &str,
                        id: Option<i64>,
                        name: &str,
                        monitor: &str,
                        on_screen: bool|
     -> usize {
        if let Some(pos) = groups.iter().position(|g| g.key == key) {
            if groups[pos].monitor.is_empty() && !monitor.is_empty() {
                groups[pos].monitor = monitor.to_string();
            }
            if on_screen {
                groups[pos].on_screen = true;
            }
            return pos;
        }
        groups.push(WorkspaceGroup {
            key: key.to_string(),
            id,
            name: name.to_string(),
            label: workspace_label(name, id),
            special: is_special_name(name),
            monitor: monitor.to_string(),
            on_screen,
            entries: Vec::new(),
        });
        groups.len() - 1
    };

    for desc in extra {
        ensure_group(
            &mut groups,
            &desc.key,
            desc.id,
            &desc.name,
            &desc.monitor,
            desc.on_screen,
        );
    }

    for (i, entry) in entries.iter().enumerate() {
        let on_screen = extra
            .iter()
            .any(|d| d.key == entry.workspace_key && d.on_screen);
        let pos = ensure_group(
            &mut groups,
            &entry.workspace_key,
            entry.workspace_id,
            &entry.workspace_name,
            &entry.monitor_name,
            on_screen,
        );
        groups[pos].entries.push(i);
    }

    groups.sort_by(|a, b| {
        let ra = workspace_rank(a);
        let rb = workspace_rank(b);
        if ra != rb {
            return ra.cmp(&rb);
        }
        if ra == 0 && a.id != b.id {
            return a.id.cmp(&b.id);
        }
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });

    groups
}

pub fn group_of(groups: &[WorkspaceGroup], index: Option<usize>) -> Option<usize> {
    let idx = index?;
    groups.iter().position(|g| g.entries.contains(&idx))
}

pub fn group_front(group: &WorkspaceGroup, order: &[usize]) -> Option<usize> {
    group
        .entries
        .iter()
        .copied()
        .find(|idx| order.contains(idx))
}

pub fn neighbor_workspace(
    groups: &[WorkspaceGroup],
    order: &[usize],
    current: Option<usize>,
    direction: isize,
) -> Option<usize> {
    let Some(at) = group_of(groups, current) else {
        return order.first().copied();
    };
    let dir: isize = if direction < 0 { -1 } else { 1 };
    let mut i = at as isize + dir;
    while i >= 0 && (i as usize) < groups.len() {
        if let Some(front) = group_front(&groups[i as usize], order) {
            return Some(front);
        }
        i += dir;
    }
    current
}

pub fn workspace_by_number(
    groups: &[WorkspaceGroup],
    order: &[usize],
    number: i64,
) -> Option<usize> {
    for group in groups {
        if !group.special
            && group.id == Some(number)
            && let Some(front) = group_front(group, order)
        {
            return Some(front);
        }
    }
    None
}

pub const PIXELS_PER_STEP: f64 = 60.0;
pub const ANGLE_PER_STEP: f64 = 120.0;

pub fn wheel_steps(carry: f64, angle_delta: f64, pixel_delta: f64) -> (isize, f64) {
    let unit = if pixel_delta != 0.0 {
        PIXELS_PER_STEP
    } else {
        ANGLE_PER_STEP
    };
    let total = carry
        + if pixel_delta != 0.0 {
            pixel_delta
        } else {
            angle_delta
        };
    let steps = if total <= -unit {
        (total / unit).ceil() as isize
    } else if total >= unit {
        (total / unit).floor() as isize
    } else {
        0
    };
    let rest = total - steps as f64 * unit;
    (if steps == 0 { 0 } else { -steps }, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_marks_match_fathom_spec() {
        let labels: Vec<String> = (0..=8).map(depth_mark_label).collect();
        assert_eq!(
            labels,
            vec!["now", "30s", "1m", "3m", "7m", "15m", "31m", "1h", "2h+"]
        );
    }

    #[test]
    fn fathom_labels_format_correctly() {
        assert_eq!(fathom_label(0.0, true, false), "");
        assert_eq!(fathom_label(1.3, false, true), "");
        assert_eq!(fathom_label(0.02, false, false), "at the surface");
        assert_eq!(fathom_label(1.0, false, false), "1 fathom");
        assert_eq!(fathom_label(1.34, false, false), "1.3 fathoms");
    }

    #[test]
    fn wheel_accumulates_fractional_deltas() {
        let (s1, r1) = wheel_steps(0.0, -60.0, 0.0);
        assert_eq!(s1, 0);
        assert_eq!(r1, -60.0);
        let (s2, r2) = wheel_steps(r1, -60.0, 0.0);
        assert_eq!(s2, 1);
        assert_eq!(r2, 0.0);
    }
}
